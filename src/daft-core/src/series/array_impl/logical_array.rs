use std::sync::Arc;

use super::{ArrayWrapper, IntoSeries, Series};
use crate::{
    array::{ops::GroupIndices, prelude::*},
    datatypes::prelude::*,
    series::{DaftResult, SeriesLike},
    with_match_integer_daft_types,
};

impl<L> IntoSeries for LogicalArray<L>
where
    L: DaftLogicalType,
    ArrayWrapper<Self>: SeriesLike,
{
    fn into_series(self) -> Series {
        Series {
            inner: Arc::new(ArrayWrapper(self)),
        }
    }
}

macro_rules! impl_series_like_for_logical_array {
    ($da:ident) => {
        impl SeriesLike for ArrayWrapper<$da> {
            fn into_series(&self) -> Series {
                self.0.clone().into_series()
            }
            fn to_arrow(&self) -> Box<dyn arrow2::array::Array> {
                self.0.to_arrow()
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn with_validity(
                &self,
                validity: Option<arrow2::bitmap::Bitmap>,
            ) -> DaftResult<Series> {
                let new_array = self.0.physical.with_validity(validity)?;
                Ok($da::new(self.0.field.clone(), new_array).into_series())
            }

            fn validity(&self) -> Option<&arrow2::bitmap::Bitmap> {
                self.0.physical.validity()
            }

            fn broadcast(&self, num: usize) -> DaftResult<Series> {
                use crate::array::ops::broadcast::Broadcastable;
                let data_array = self.0.physical.broadcast(num)?;
                Ok($da::new(self.0.field.clone(), data_array).into_series())
            }

            fn cast(&self, datatype: &DataType) -> DaftResult<Series> {
                self.0.cast(datatype)
            }

            fn data_type(&self) -> &DataType {
                self.0.data_type()
            }

            fn field(&self) -> &Field {
                self.0.field()
            }

            fn filter(&self, mask: &crate::datatypes::BooleanArray) -> DaftResult<Series> {
                let new_array = self.0.physical.filter(mask)?;
                Ok($da::new(self.0.field.clone(), new_array).into_series())
            }

            fn head(&self, num: usize) -> DaftResult<Series> {
                self.slice(0, num)
            }

            fn if_else(&self, other: &Series, predicate: &Series) -> DaftResult<Series> {
                let physical_if_else = self
                    .0
                    .physical
                    .if_else(&other.downcast::<$da>()?.physical, predicate.downcast()?)?;
                Ok($da::new(self.0.field.clone(), physical_if_else).into_series())
            }

            fn is_null(&self) -> DaftResult<Series> {
                use crate::array::ops::DaftIsNull;

                Ok(DaftIsNull::is_null(&self.0.physical)?.into_series())
            }

            fn not_null(&self) -> DaftResult<Series> {
                use crate::array::ops::DaftNotNull;

                Ok(DaftNotNull::not_null(&self.0.physical)?.into_series())
            }

            fn len(&self) -> usize {
                self.0.physical.len()
            }

            fn size_bytes(&self) -> DaftResult<usize> {
                self.0.physical.size_bytes()
            }

            fn name(&self) -> &str {
                self.0.field.name.as_str()
            }

            fn rename(&self, name: &str) -> Series {
                let new_array = self.0.physical.rename(name);
                let new_field = self.0.field.rename(name);
                $da::new(new_field, new_array).into_series()
            }

            fn slice(&self, start: usize, end: usize) -> DaftResult<Series> {
                let new_array = self.0.physical.slice(start, end)?;
                Ok($da::new(self.0.field.clone(), new_array).into_series())
            }

            fn sort(&self, descending: bool, nulls_first: bool) -> DaftResult<Series> {
                Ok(self.0.sort(descending, nulls_first)?.into_series())
            }

            fn str_value(&self, idx: usize) -> DaftResult<String> {
                self.0.str_value(idx)
            }

            fn take(&self, idx: &Series) -> DaftResult<Series> {
                with_match_integer_daft_types!(idx.data_type(), |$S| {
                    Ok(self
                        .0
                        .take(idx.downcast::<<$S as DaftDataType>::ArrayType>()?)?
                        .into_series())
                })
            }

            fn min(&self, groups: Option<&GroupIndices>) -> DaftResult<Series> {
                use crate::array::ops::DaftCompareAggable;
                let data_array = match groups {
                    Some(groups) => DaftCompareAggable::grouped_min(&self.0.physical, groups)?,
                    None => DaftCompareAggable::min(&self.0.physical)?,
                };
                Ok($da::new(self.0.field.clone(), data_array).into_series())
            }
            fn max(&self, groups: Option<&GroupIndices>) -> DaftResult<Series> {
                use crate::array::ops::DaftCompareAggable;
                let data_array = match groups {
                    Some(groups) => DaftCompareAggable::grouped_max(&self.0.physical, groups)?,
                    None => DaftCompareAggable::max(&self.0.physical)?,
                };
                Ok($da::new(self.0.field.clone(), data_array).into_series())
            }

            fn agg_list(&self, groups: Option<&GroupIndices>) -> DaftResult<Series> {
                use crate::array::{ops::DaftListAggable, ListArray};
                let data_array = match groups {
                    Some(groups) => self.0.physical.grouped_list(groups)?,
                    None => self.0.physical.list()?,
                };
                let new_field = self.field().to_list_field()?;
                Ok(ListArray::new(
                    new_field,
                    data_array.flat_child.cast(self.data_type())?,
                    data_array.offsets().clone(),
                    data_array.validity().cloned(),
                )
                .into_series())
            }

            fn agg_set(&self, groups: Option<&GroupIndices>) -> DaftResult<Series> {
                use crate::array::{ops::DaftSetAggable, ListArray};
                let data_array = match groups {
                    Some(groups) => self.0.physical.clone().into_series().grouped_set(groups)?,
                    None => self.0.physical.clone().into_series().set()?,
                };
                let new_field = self.field().to_list_field()?;
                Ok(ListArray::new(
                    new_field,
                    data_array.flat_child.cast(self.data_type())?,
                    data_array.offsets().clone(),
                    data_array.validity().cloned(),
                )
                .into_series())
            }
        }
    };
}

impl_series_like_for_logical_array!(DateArray);
impl_series_like_for_logical_array!(TimeArray);
impl_series_like_for_logical_array!(DurationArray);

impl_series_like_for_logical_array!(TimestampArray);
impl_series_like_for_logical_array!(ImageArray);
impl_series_like_for_logical_array!(FixedShapeImageArray);
impl_series_like_for_logical_array!(TensorArray);
impl_series_like_for_logical_array!(EmbeddingArray);
impl_series_like_for_logical_array!(FixedShapeTensorArray);
impl_series_like_for_logical_array!(SparseTensorArray);
impl_series_like_for_logical_array!(FixedShapeSparseTensorArray);
impl_series_like_for_logical_array!(MapArray);
