mod explode;
mod join;
pub use explode::explode;
pub use join::list_join;

#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
pub fn register_modules(parent: &Bound<PyModule>) -> PyResult<()> {
    parent.add_function(wrap_pyfunction_bound!(explode::py_explode, parent)?)?;
    parent.add_function(wrap_pyfunction_bound!(join::py_list_join, parent)?)?;
    Ok(())
}
