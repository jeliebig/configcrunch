#![feature(path_try_exists)]
#![feature(in_band_lifetimes)]
#![feature(trait_alias)]

use pyo3::prelude::*;
use pyo3::{PyResult, Python, wrap_pyfunction};

pub(crate) const REF: &str = "$ref";
pub(crate) const REMOVE: &str = "$remove";
pub(crate) const REMOVE_FROM_LIST_PREFIX: &str = "$remove::";
pub(crate) const FORCE_STRING: &str = "__forcestring__";

pub(crate) mod errors;
pub(crate) mod loader;
pub(crate) mod merger;
pub(crate) mod variables;
pub(crate) mod ycd;
mod minijinja;
mod conv;

use crate::errors::*;
use crate::loader::*;
use crate::merger::*;
use crate::ycd::*;

#[pymodule]
fn _main(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add("ConfigcrunchError", py.get_type::<ConfigcrunchError>())?;
    m.add("ReferencedDocumentNotFound", py.get_type::<ReferencedDocumentNotFound>())?;
    m.add("CircularDependencyError", py.get_type::<CircularDependencyError>())?;
    m.add("VariableProcessingError", py.get_type::<VariableProcessingError>())?;
    m.add("InvalidDocumentError", py.get_type::<InvalidDocumentError>())?;
    m.add("InvalidHeaderError", py.get_type::<InvalidHeaderError>())?;
    m.add("InvalidRemoveError", py.get_type::<InvalidRemoveError>())?;

    m.add_function(wrap_pyfunction!(load_multiple_yml, m)?)?;
    m.add_function(wrap_pyfunction!(test_subdoc_specs, m)?)?;

    m.add_class::<YamlConfigDocument>()?;
    m.add_class::<DocReference>()?;

    Ok(())
}