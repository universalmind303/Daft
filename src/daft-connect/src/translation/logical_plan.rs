use std::sync::Arc;

use daft_core::prelude::Schema;
use daft_dsl::LiteralValue;
use daft_logical_plan::{LogicalPlanBuilder, PyLogicalPlanBuilder};
use daft_micropartition::{
    partitioning::{
        MicroPartitionSet, PartitionCacheEntry, PartitionMetadata, PartitionSet, PartitionSetCache,
    },
    python::PyMicroPartition,
    MicroPartition,
};
use daft_table::Table;
use eyre::{bail, Context};
use futures::TryStreamExt;
use pyo3::Python;
use spark_connect::{relation::RelType, Limit, Relation, ShowString};
use tracing::warn;

use crate::{session::Session, Runner};

mod aggregate;
mod drop;
mod filter;
mod local_relation;
mod project;
mod range;
mod read;
mod to_df;
mod with_columns;
mod with_columns_renamed;

use pyo3::prelude::*;

pub struct SparkAnalyzer<'a> {
    pub session: &'a Session,
}
impl SparkAnalyzer<'_> {
    pub fn new<'a>(session: &'a Session) -> SparkAnalyzer<'a> {
        SparkAnalyzer { session }
    }

    pub fn create_in_memory_scan(
        &self,
        plan_id: usize,
        schema: Arc<Schema>,
        tables: Vec<Table>,
    ) -> eyre::Result<LogicalPlanBuilder> {
        let runner = self.session.get_runner()?;

        match runner {
            Runner::Ray => {
                let mp =
                    MicroPartition::new_loaded(tables[0].schema.clone(), Arc::new(tables), None);
                Python::with_gil(|py| {
                    // Convert MicroPartition to a logical plan using Python interop.
                    let py_micropartition = py
                        .import_bound(pyo3::intern!(py, "daft.table"))?
                        .getattr(pyo3::intern!(py, "MicroPartition"))?
                        .getattr(pyo3::intern!(py, "_from_pymicropartition"))?
                        .call1((PyMicroPartition::from(mp),))?;

                    // ERROR:   2: AttributeError: 'daft.daft.PySchema' object has no attribute '_schema'
                    let py_plan_builder = py
                        .import_bound(pyo3::intern!(py, "daft.dataframe.dataframe"))?
                        .getattr(pyo3::intern!(py, "to_logical_plan_builder"))?
                        .call1((py_micropartition,))?;
                    let py_plan_builder = py_plan_builder.getattr(pyo3::intern!(py, "_builder"))?;
                    let plan: PyLogicalPlanBuilder = py_plan_builder.extract()?;
                    
                    Ok::<_, eyre::Error>(dbg!(plan.builder))
                })
            }
            Runner::Native => {
                let partition_key = uuid::Uuid::new_v4().to_string();

                let pset = Arc::new(MicroPartitionSet::from_tables(plan_id, tables)?);

                let PartitionMetadata {
                    num_rows,
                    size_bytes,
                } = pset.metadata();
                let num_partitions = pset.num_partitions();

                self.session.psets.put_partition_set(&partition_key, &pset);

                let cache_entry = PartitionCacheEntry::new_rust(partition_key.clone(), pset);

                Ok(LogicalPlanBuilder::in_memory_scan(
                    &partition_key,
                    cache_entry,
                    schema,
                    num_partitions,
                    size_bytes,
                    num_rows,
                )?)
            }
        }
    }

    pub async fn to_logical_plan(&self, relation: Relation) -> eyre::Result<LogicalPlanBuilder> {
        let Some(common) = relation.common else {
            bail!("Common metadata is required");
        };

        if common.origin.is_some() {
            warn!("Ignoring common metadata for relation: {common:?}; not yet implemented");
        }

        let Some(rel_type) = relation.rel_type else {
            bail!("Relation type is required");
        };

        match rel_type {
            RelType::Limit(l) => self
                .limit(*l)
                .await
                .wrap_err("Failed to apply limit to logical plan"),
            RelType::Range(r) => self
                .range(r)
                .wrap_err("Failed to apply range to logical plan"),
            RelType::Project(p) => self
                .project(*p)
                .await
                .wrap_err("Failed to apply project to logical plan"),
            RelType::Aggregate(a) => self
                .aggregate(*a)
                .await
                .wrap_err("Failed to apply aggregate to logical plan"),
            RelType::WithColumns(w) => self
                .with_columns(*w)
                .await
                .wrap_err("Failed to apply with_columns to logical plan"),
            RelType::ToDf(t) => self
                .to_df(*t)
                .await
                .wrap_err("Failed to apply to_df to logical plan"),
            RelType::LocalRelation(l) => {
                let Some(plan_id) = common.plan_id else {
                    bail!("Plan ID is required for LocalRelation");
                };
                self.local_relation(plan_id, l)
                    .wrap_err("Failed to apply local_relation to logical plan")
            }
            RelType::WithColumnsRenamed(w) => self
                .with_columns_renamed(*w)
                .await
                .wrap_err("Failed to apply with_columns_renamed to logical plan"),
            RelType::Read(r) => read::read(r)
                .await
                .wrap_err("Failed to apply read to logical plan"),
            RelType::Drop(d) => self
                .drop(*d)
                .await
                .wrap_err("Failed to apply drop to logical plan"),
            RelType::Filter(f) => self
                .filter(*f)
                .await
                .wrap_err("Failed to apply filter to logical plan"),
            RelType::ShowString(ss) => {
                let Some(plan_id) = common.plan_id else {
                    bail!("Plan ID is required for LocalRelation");
                };
                self.show_string(plan_id, *ss)
                    .await
                    .wrap_err("Failed to show string")
            }
            plan => bail!("Unsupported relation type: {plan:?}"),
        }
    }

    async fn limit(&self, limit: Limit) -> eyre::Result<LogicalPlanBuilder> {
        let Limit { input, limit } = limit;

        let Some(input) = input else {
            bail!("input must be set");
        };

        let plan = Box::pin(self.to_logical_plan(*input)).await?;

        plan.limit(i64::from(limit), false)
            .wrap_err("Failed to apply limit to logical plan")
    }

    /// right now this just naively applies a limit to the logical plan
    /// In the future, we want this to more closely match our daft implementation
    async fn show_string(
        &self,
        plan_id: i64,
        show_string: ShowString,
    ) -> eyre::Result<LogicalPlanBuilder> {
        let ShowString {
            input,
            num_rows,
            truncate: _,
            vertical,
        } = show_string;

        if vertical {
            bail!("Vertical show string is not supported");
        }

        let Some(input) = input else {
            bail!("input must be set");
        };

        let plan = Box::pin(self.to_logical_plan(*input)).await?;
        let plan = plan.limit(num_rows as i64, true)?;

        let results = self.session.run_query(plan).await?;
        let results = results.try_collect::<Vec<_>>().await?;
        let single_batch = results
            .into_iter()
            .next()
            .ok_or_else(|| eyre::eyre!("No results"))?;

        let tbls = single_batch.get_tables()?;
        let tbl = Table::concat(&tbls)?;
        let output = tbl.to_comfy_table(None).to_string();

        let s = LiteralValue::Utf8(output)
            .into_single_value_series()?
            .rename("show_string");

        let tbl = Table::from_nonempty_columns(vec![s])?;
        let schema = tbl.schema.clone();

        self.create_in_memory_scan(plan_id as _, schema, vec![tbl])
    }
}
