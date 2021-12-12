use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use minijinja::{Environment, Error, ErrorKind, State};
use minijinja::value::{Object, Primitive, Value};
use pyo3::{Py, PyCell, PyObject, PyResult, Python, ToPyObject};
use pyo3::types::PyTuple;
use crate::conv::{PyYamlConfigDocument, SimpleYcdValueType, YcdValueType, YHashMap};
use crate::{FORCE_STRING, YamlConfigDocument};

// https://github.com/rust-lang/rust/issues/70263
macro_rules! typed_closure {
    (($($bound:tt)*), $closure:expr) => { {
        fn _typed_closure_id<F>(f: F) -> F where F: $($bound)* { f }
        _typed_closure_id($closure)
    } }
}

trait FuncFunc = Fn(&State, Vec<Value>) -> Result<Value, Error> + Sync + Send + 'static;

pub(crate) struct TemplateRenderer<'env> {
    env: Environment<'env>,
    document: PyYamlConfigDocument,
    globals: HashMap<String, Box<dyn FuncFunc>>
}

impl<'env> TemplateRenderer<'env> {
    const STR_FILTER: &'static str = "str";
    const TPL_NAME: &'static str = "tpl";

    pub(crate) fn new(dref: &'env PyCell<YamlConfigDocument>) -> PyResult<Self> {
        if dref.borrow().bound_helpers.is_empty() {
            YamlConfigDocument::collect_bound_variable_helpers(dref, dref.py())?;
        }
        let mut slf = Self {
            env: Environment::new(), document: Py::from(dref).into(), globals: HashMap::new()
        };

        slf.env.add_filter(Self::STR_FILTER, str_filter);

        Ok(slf)
    }

    pub(crate) fn render(mut self, py: Python<'env>, helpers: &'env HashMap<String, PyObject>, input: &'env str) -> Result<Option<String>, Error> {
        if !input.contains('{') {
            // Shortcut if it doesn't contain any variables or control structures
            return Ok(None)
        }
        for (name, helper) in helpers {
            self.env.add_function(name, TemplateRenderer::create_helper_fn(helper.clone_ref(py)))
        }
        self.env.add_template(Self::TPL_NAME, input)?;
        let result = self.env
            .get_template(Self::TPL_NAME)?
            .render_from_value(Self::build_context(self.document.clone_ref(py)))?;
        self.env.remove_template(Self::TPL_NAME);
        Ok(Some(result))
    }

    pub(crate) fn add_helpers(&mut self, py: Python, helpers: Vec<PyObject>) {
        self.globals.extend(helpers
            .into_iter()
            .map(|f| (
                f.getattr(py, "__name__").unwrap().extract(py).unwrap(),
                Self::create_helper_fn(f)
            ))
        );
    }

    #[inline]
    fn build_context(document: PyYamlConfigDocument) -> Value {
        Value::from_object(document)
    }

    pub fn create_helper_fn(pyf: PyObject) -> Box<dyn FuncFunc> {
         Box::new(typed_closure!((FuncFunc), move |_state: &State, args: Vec<Value>| -> Result<Value, Error> {
             Python::with_gil(|py| {
                 let pyargs = PyTuple::new(py, args.into_iter().map(WValue));

                 match pyf.call1(py, pyargs) {
                     Ok(v) => {
                         match v.extract::<YcdValueType>(py) {
                             Ok(ycdvalue) => {
                                 Ok(ycdvalue.into())
                             }
                             Err(e) => convert_pyerr(e)
                         }
                     },
                     Err(e) => convert_pyerr(e)
                 }
             })
        }))
    }
}

impl Display for PyYamlConfigDocument {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Python::with_gil(|py| match YamlConfigDocument::__str__(self.0.clone_ref(py), py) {
            Ok(v) => write!(f, "{}", v),
            Err(_) => write!(f, "YCD<?? Error during Display ??>", )
        })
    }
}

fn str_filter(_state: &State, value: String) -> Result<String, Error> {
    Ok(Value::from(format!("{}{}", FORCE_STRING, value)).to_string())
}

fn convert_pyerr<_T>(in_e: pyo3::PyErr) -> Result<_T, Error> {
    Err(Error::new(ErrorKind::ImpossibleOperation, format!(
        "Error in a function: {:?}", in_e
    )))
}

impl From<SimpleYcdValueType> for Value {
    fn from(in_v: SimpleYcdValueType) -> Self {
        match in_v {
            SimpleYcdValueType::Dict(v) => Value::from_serializable(&v),
            SimpleYcdValueType::List(v) => Value::from(v),
            SimpleYcdValueType::YString(v) => Value::from(v),
            SimpleYcdValueType::Bool(v) => Value::from(v),
            SimpleYcdValueType::Int(v) => Value::from(v),
            SimpleYcdValueType::Float(v) => Value::from(v)
        }
    }
}

impl From<YcdValueType> for Value {
    fn from(in_v: YcdValueType) -> Self {
        match in_v {
            YcdValueType::Dict(v) => Value::from_object(YHashMap(v)),
            YcdValueType::List(v) => v.into_iter().map(|v| v.into()).collect::<Vec<Value>>().into(),
            YcdValueType::YString(v) => Value::from(v),
            YcdValueType::Bool(v) => Value::from(v),
            YcdValueType::Int(v) => Value::from(v),
            YcdValueType::Float(v) => Value::from(v),
            YcdValueType::Ycd(v) => Value::from_object(v)
        }
    }
}

impl From<&YcdValueType> for Value {
    fn from(in_v: &YcdValueType) -> Self {
        match in_v {
            YcdValueType::Dict(v) => Value::from_object(YHashMap(v.clone())), // TODO: Not ideal
            YcdValueType::List(v) => v.iter().map(|v| v.into()).collect::<Vec<Value>>().into(),
            YcdValueType::YString(v) => Value::from(v.clone()),
            YcdValueType::Bool(v) => Value::from(*v),
            YcdValueType::Int(v) => Value::from(*v),
            YcdValueType::Float(v) => Value::from(*v),
            YcdValueType::Ycd(v) => Python::with_gil(|py| Value::from_object(v.clone_ref(py)))
        }
    }
}

struct WValue(Value);
impl ToPyObject for WValue {
    fn to_object(&self, py: Python) -> PyObject {
        match self.0.as_primitive() {
            None => py.None(),
            Some(v) => match v {
                Primitive::Undefined => py.None(),
                Primitive::None => py.None(),
                Primitive::Bool(v) => v.to_object(py),
                Primitive::U64(v) => v.to_object(py),
                Primitive::U128(v) => v.to_object(py),
                Primitive::I64(v) => v.to_object(py),
                Primitive::I128(v) => v.to_object(py),
                Primitive::F64(v) => v.to_object(py),
                Primitive::Char(v) => v.to_object(py),
                Primitive::Str(v) => v.to_object(py),
                Primitive::Bytes(v) => v.to_object(py)
            }
        }
    }
}

impl Object for PyYamlConfigDocument {
    fn get_attr(&self, name: &str) -> Option<Value> {
        Python::with_gil(|py| {
            let bow = self.0.borrow(py);
            bow.doc.get(name).map(|x| x.into())
        })
    }

    fn call_method(
        &self, state: &State, name: &str, args: Vec<Value>
    ) -> Result<Value, Error> {
        Python::with_gil(|py| {
            let mut bow = self.0.borrow(py);
            if bow.bound_helpers.is_empty() {
                drop(bow);
                YamlConfigDocument::collect_bound_variable_helpers(self.0.clone_ref(py).as_ref(py), py)
                    .map_err(|e| convert_pyerr::<bool>(e).unwrap_err())?;
                bow = self.0.borrow(py);
            }
            match bow.bound_helpers.get(name) {
                None => Err(Error::new(ErrorKind::ImpossibleOperation, format!("Method {} not found on object", name))),
                Some(helper) => TemplateRenderer::create_helper_fn(helper.clone_ref(py))(state, args)
            }
        })
    }
}

impl Object for YHashMap<String, YcdValueType> {
    fn get_attr(&self, name: &str) -> Option<Value> {
        self.0.get(name).map(|x| x.into())
    }
}
