mod chunk;
mod count;
mod explode;
mod join;
pub use chunk::list_chunk;
pub use count::list_count;
pub use explode::explode;
pub use join::list_join;

#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
pub fn register_modules(parent: &Bound<PyModule>) -> PyResult<()> {
    parent.add_function(wrap_pyfunction_bound!(chunk::py_list_chunk, parent)?)?;
    parent.add_function(wrap_pyfunction_bound!(count::py_list_count, parent)?)?;
    parent.add_function(wrap_pyfunction_bound!(explode::py_explode, parent)?)?;
    parent.add_function(wrap_pyfunction_bound!(join::py_list_join, parent)?)?;
    Ok(())
}
