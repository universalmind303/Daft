use std::{
    collections::HashMap,
    ffi::CStr,
    fs::File,
    io::Write,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use common_daft_config::DaftExecutionConfig;
use common_display::{mermaid::MermaidDisplayOptions, DisplayLevel};
use common_error::DaftResult;
use common_tracing::refresh_chrome_trace;
use daft_local_plan::translate;
use daft_logical_plan::LogicalPlanBuilder;
use daft_micropartition::{
    partitioning::{InMemoryPartitionSetCache, MicroPartitionSet},
    MicroPartition,
};
use futures::{FutureExt, Stream};
use loole::RecvFuture;
use tokio_util::sync::CancellationToken;
#[cfg(feature = "python")]
use {
    common_daft_config::PyDaftExecutionConfig,
    daft_logical_plan::PyLogicalPlanBuilder,
    daft_micropartition::python::PyMicroPartition,
    pyo3::{
        ffi::c_str,
        intern, pyclass, pymethods,
        types::{PyAnyMethods, PyDict},
        Bound, IntoPyObject, PyAny, PyObject, PyRef, PyRefMut, PyResult, Python,
    },
};

use crate::{
    channel::{create_channel, Receiver},
    pipeline::{physical_plan_to_pipeline, viz_pipeline_ascii, viz_pipeline_mermaid},
    progress_bar::{make_progress_bar_manager, ProgressBarManager},
    resource_manager::get_or_init_memory_manager,
    Error, ExecutionRuntimeContext,
};

#[cfg(feature = "python")]
#[pyclass]
struct LocalPartitionIterator {
    iter: Box<dyn Iterator<Item = DaftResult<PyObject>> + Send + Sync>,
}

#[cfg(feature = "python")]
#[pymethods]
impl LocalPartitionIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(mut slf: PyRefMut<'_, Self>, py: Python) -> PyResult<Option<PyObject>> {
        let iter = &mut slf.iter;
        Ok(py.allow_threads(|| iter.next().transpose())?)
    }
}

#[cfg(feature = "python")]
#[pyclass(module = "daft.daft", name = "NativeExecutor")]
pub struct PyNativeExecutor {
    executor: NativeExecutor,
    part_set_cache: Arc<PyObject>,
}

#[cfg(feature = "python")]
#[pymethods]
impl PyNativeExecutor {
    #[new]
    pub fn new(py: Python) -> PyResult<Self> {
        // from daft.runners.runner import LOCAL_PARTITION_SET_CACHE
        let module = py.import(intern!(py, "daft.runners.runner"))?;
        let local_partition_set_cache = module.getattr(intern!(py, "LOCAL_PARTITION_SET_CACHE"))?;
        let local_partition_set_cache = local_partition_set_cache.unbind();
        let local_partition_set_cache = Arc::new(local_partition_set_cache);

        Ok(Self {
            executor: NativeExecutor::new(),
            part_set_cache: local_partition_set_cache,
        })
    }

    #[pyo3(signature = (logical_plan_builder, cfg, results_buffer_size=None))]
    pub fn run<'a>(
        &mut self,
        py: Python<'a>,
        logical_plan_builder: &PyLogicalPlanBuilder,
        cfg: PyDaftExecutionConfig,
        results_buffer_size: Option<usize>,
    ) -> PyResult<Bound<'a, PyAny>> {
        // i don't know how to do this in pyo3/rust, so we just `eval` the python code
        let psets = {
            const CODE_1: &CStr = c_str!(
                "{k: v.values() for k, v in part_set_cache.get_all_partition_sets().items()}"
            );

            const CODE_2: &CStr = c_str!(
                "{part_id: [part.micropartition()._micropartition for part in parts] for part_id, parts in psets.items()}"
            );

            let locals = PyDict::new(py);

            locals.set_item(intern!(py, "part_set_cache"), self.part_set_cache.as_ref())?;

            let res = py.eval(CODE_1, None, Some(&locals))?;
            let locals = PyDict::new(py);
            locals.set_item(intern!(py, "psets"), res)?;

            py.eval(CODE_2, None, Some(&locals))
        }?;
        let psets = psets.extract::<HashMap<String, Vec<PyMicroPartition>>>()?;

        let native_psets: HashMap<String, Arc<MicroPartitionSet>> = psets
            .into_iter()
            .map(|(part_id, parts)| {
                (
                    part_id,
                    Arc::new(
                        parts
                            .into_iter()
                            .map(std::convert::Into::into)
                            .collect::<Vec<Arc<MicroPartition>>>()
                            .into(),
                    ),
                )
            })
            .collect();
        let psets = InMemoryPartitionSetCache::new(&native_psets);
        let out = py.allow_threads(|| {
            self.executor.psets.copy_from(psets);
            self.executor
                .run(
                    &logical_plan_builder.builder,
                    cfg.config,
                    results_buffer_size,
                )
                .map(|res| res.into_iter())
        })?;
        let iter = Box::new(out.map(|part| {
            pyo3::Python::with_gil(|py| {
                Ok(PyMicroPartition::from(part?)
                    .into_pyobject(py)?
                    .unbind()
                    .into_any())
            })
        }));
        let part_iter = LocalPartitionIterator { iter };
        Ok(part_iter.into_pyobject(py)?.into_any())
    }

    pub fn repr_ascii(
        &self,
        logical_plan_builder: &PyLogicalPlanBuilder,
        cfg: PyDaftExecutionConfig,
        simple: bool,
    ) -> PyResult<String> {
        Ok(self
            .executor
            .repr_ascii(&logical_plan_builder.builder, cfg.config, simple))
    }

    pub fn repr_mermaid(
        &self,
        logical_plan_builder: &PyLogicalPlanBuilder,
        cfg: PyDaftExecutionConfig,
        options: MermaidDisplayOptions,
    ) -> PyResult<String> {
        Ok(self
            .executor
            .repr_mermaid(&logical_plan_builder.builder, cfg.config, options))
    }

    pub fn get_partition_set_cache(&self) -> PyResult<&PyObject> {
        Ok(self.part_set_cache.as_ref())
    }

    pub fn get_partition_set(&self, py: Python, pset_id: &str) -> PyResult<PyObject> {
        self.part_set_cache
            .call_method(py, "get_partition_set", (pset_id,), None)
    }

    pub fn put_partition_set(&self, py: Python, pset: PyObject) -> PyResult<PyObject> {
        self.part_set_cache
            .call_method(py, "put_partition_set", (pset,), None)
    }
}

#[derive(Debug, Clone)]
pub struct NativeExecutor {
    pub psets: InMemoryPartitionSetCache,
    cancel: CancellationToken,
    runtime: Option<Arc<tokio::runtime::Runtime>>,
    pb_manager: Option<Arc<dyn ProgressBarManager>>,
    enable_explain_analyze: bool,
}

impl Default for NativeExecutor {
    fn default() -> Self {
        Self {
            psets: InMemoryPartitionSetCache::empty(),
            cancel: CancellationToken::new(),
            runtime: None,
            pb_manager: should_enable_progress_bar().then(make_progress_bar_manager),
            enable_explain_analyze: should_enable_explain_analyze(),
        }
    }
}

impl NativeExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_runtime(mut self, runtime: Arc<tokio::runtime::Runtime>) -> Self {
        self.runtime = Some(runtime);
        self
    }

    pub fn with_progress_bar_manager(mut self, pb_manager: Arc<dyn ProgressBarManager>) -> Self {
        self.pb_manager = Some(pb_manager);
        self
    }

    pub fn enable_explain_analyze(mut self, b: bool) -> Self {
        self.enable_explain_analyze = b;
        self
    }

    pub fn run(
        &self,
        logical_plan_builder: &LogicalPlanBuilder,
        cfg: Arc<DaftExecutionConfig>,
        results_buffer_size: Option<usize>,
    ) -> DaftResult<ExecutionEngineResult> {
        let logical_plan = logical_plan_builder.build();
        let physical_plan = translate(&logical_plan)?;

        refresh_chrome_trace();
        let cancel = self.cancel.clone();
        let pipeline = physical_plan_to_pipeline(&physical_plan, &self.psets, &cfg)?;
        let (tx, rx) = create_channel(results_buffer_size.unwrap_or(1));

        let rt = self.runtime.clone();
        let pb_manager = self.pb_manager.clone();
        let enable_explain_analyze = self.enable_explain_analyze;
        // todo: split this into a run and run_async method
        // the run_async should spawn a task instead of a thread like this
        let handle = std::thread::spawn(move || {
            let runtime = rt.unwrap_or_else(|| {
                Arc::new(
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("Failed to create tokio runtime"),
                )
            });
            let execution_task = async {
                let memory_manager = get_or_init_memory_manager();
                let mut runtime_handle = ExecutionRuntimeContext::new(
                    cfg.default_morsel_size,
                    memory_manager.clone(),
                    pb_manager,
                );
                let receiver = pipeline.start(true, &mut runtime_handle)?;

                while let Some(val) = receiver.recv().await {
                    if tx.send(val).await.is_err() {
                        break;
                    }
                }

                while let Some(result) = runtime_handle.join_next().await {
                    match result {
                        Ok(Err(e)) => {
                            runtime_handle.shutdown().await;
                            return DaftResult::Err(e.into());
                        }
                        Err(e) => {
                            runtime_handle.shutdown().await;
                            return DaftResult::Err(Error::JoinError { source: e }.into());
                        }
                        _ => {}
                    }
                }
                if enable_explain_analyze {
                    let curr_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("Time went backwards")
                        .as_millis();
                    let file_name = format!("explain-analyze-{curr_ms}-mermaid.md");
                    let mut file = File::create(file_name)?;
                    writeln!(
                        file,
                        "```mermaid\n{}\n```",
                        viz_pipeline_mermaid(
                            pipeline.as_ref(),
                            DisplayLevel::Verbose,
                            true,
                            Default::default()
                        )
                    )?;
                }
                Ok(())
            };

            let local_set = tokio::task::LocalSet::new();
            local_set.block_on(&runtime, async {
                tokio::select! {
                    biased;
                    () = cancel.cancelled() => {
                        log::info!("Execution engine cancelled");
                        Ok(())
                    }
                    _ = tokio::signal::ctrl_c() => {
                        log::info!("Received Ctrl-C, shutting down execution engine");
                        Ok(())
                    }
                    result = execution_task => result,
                }
            })
        });

        Ok(ExecutionEngineResult {
            handle,
            receiver: rx,
        })
    }

    fn repr_ascii(
        &self,
        logical_plan_builder: &LogicalPlanBuilder,
        cfg: Arc<DaftExecutionConfig>,
        simple: bool,
    ) -> String {
        let logical_plan = logical_plan_builder.build();
        let physical_plan = translate(&logical_plan).unwrap();
        let pipeline_node =
            physical_plan_to_pipeline(&physical_plan, &InMemoryPartitionSetCache::empty(), &cfg)
                .unwrap();

        viz_pipeline_ascii(pipeline_node.as_ref(), simple)
    }

    fn repr_mermaid(
        &self,
        logical_plan_builder: &LogicalPlanBuilder,
        cfg: Arc<DaftExecutionConfig>,
        options: MermaidDisplayOptions,
    ) -> String {
        let logical_plan = logical_plan_builder.build();
        let physical_plan = translate(&logical_plan).unwrap();
        let pipeline_node =
            physical_plan_to_pipeline(&physical_plan, &InMemoryPartitionSetCache::empty(), &cfg)
                .unwrap();

        let display_type = if options.simple {
            DisplayLevel::Compact
        } else {
            DisplayLevel::Default
        };
        viz_pipeline_mermaid(
            pipeline_node.as_ref(),
            display_type,
            options.bottom_up,
            options.subgraph_options,
        )
    }
}

impl Drop for NativeExecutor {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn should_enable_explain_analyze() -> bool {
    let explain_var_name = "DAFT_DEV_ENABLE_EXPLAIN_ANALYZE";
    if let Ok(val) = std::env::var(explain_var_name)
        && matches!(val.trim().to_lowercase().as_str(), "1" | "true")
    {
        true
    } else {
        false
    }
}

fn should_enable_progress_bar() -> bool {
    let progress_var_name = "DAFT_PROGRESS_BAR";
    if let Ok(val) = std::env::var(progress_var_name) {
        matches!(val.trim().to_lowercase().as_str(), "1" | "true")
    } else {
        true // Return true when env var is not set
    }
}

pub struct ExecutionEngineReceiverIterator {
    receiver: Receiver<Arc<MicroPartition>>,
    handle: Option<std::thread::JoinHandle<DaftResult<()>>>,
}

impl Iterator for ExecutionEngineReceiverIterator {
    type Item = DaftResult<Arc<MicroPartition>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.receiver.blocking_recv() {
            Some(part) => Some(Ok(part)),
            None => {
                if self.handle.is_some() {
                    let join_result = self
                        .handle
                        .take()
                        .unwrap()
                        .join()
                        .expect("Execution engine thread panicked");
                    match join_result {
                        Ok(()) => None,
                        Err(e) => Some(Err(e)),
                    }
                } else {
                    None
                }
            }
        }
    }
}

pub struct ExecutionEngineReceiverStream {
    receive_fut: RecvFuture<Arc<MicroPartition>>,
    handle: Option<std::thread::JoinHandle<DaftResult<()>>>,
}

impl Stream for ExecutionEngineReceiverStream {
    type Item = DaftResult<Arc<MicroPartition>>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.receive_fut.poll_unpin(cx) {
            std::task::Poll::Ready(Ok(part)) => std::task::Poll::Ready(Some(Ok(part))),
            std::task::Poll::Ready(Err(_)) => {
                if self.handle.is_some() {
                    let join_result = self
                        .handle
                        .take()
                        .unwrap()
                        .join()
                        .expect("Execution engine thread panicked");
                    match join_result {
                        Ok(()) => std::task::Poll::Ready(None),
                        Err(e) => std::task::Poll::Ready(Some(Err(e))),
                    }
                } else {
                    std::task::Poll::Ready(None)
                }
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

pub struct ExecutionEngineResult {
    handle: std::thread::JoinHandle<DaftResult<()>>,
    receiver: Receiver<Arc<MicroPartition>>,
}

impl ExecutionEngineResult {
    pub fn into_stream(self) -> impl Stream<Item = DaftResult<Arc<MicroPartition>>> {
        ExecutionEngineReceiverStream {
            receive_fut: self.receiver.into_inner().recv_async(),
            handle: Some(self.handle),
        }
    }
}

impl IntoIterator for ExecutionEngineResult {
    type Item = DaftResult<Arc<MicroPartition>>;
    type IntoIter = ExecutionEngineReceiverIterator;

    fn into_iter(self) -> Self::IntoIter {
        ExecutionEngineReceiverIterator {
            receiver: self.receiver,
            handle: Some(self.handle),
        }
    }
}
