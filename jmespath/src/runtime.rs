use std::collections::HashMap;
use std::sync::Arc;

use crate::functions::*;
use crate::parse;
use crate::Expression;
use crate::JmespathError;

/// Compiles JMESPath expressions.
///
/// Most use cases don't need to worry about how Runtime works.
/// You really only need to create your own Runtimes if you are
/// utilizing custom functions in your expressions.
pub struct Runtime {
    // functions: HashMap<String, Box<dyn Function>>,
    functions: HashMap<String, Arc<dyn Function>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime {
            functions: HashMap::with_capacity(26),
        }
    }
}

impl Runtime {
    /// Creates a new Runtime.
    pub fn new() -> Runtime {
        Default::default()
    }

    /// Creates a new JMESPath expression from an expression string.
    ///
    /// The provided expression is expected to adhere to the JMESPath
    /// grammar: <https://jmespath.org/specification.html>
    #[inline]
    pub fn compile<'a>(&'a self, expression: &str) -> Result<Expression<'a>, JmespathError> {
        parse(expression).map(|ast| Expression::new(expression, ast, self))
    }

    /// Adds a new function to the runtime.
    #[inline]
    pub fn register_function(&mut self, name: &str, f: Arc<dyn Function>) {
        self.functions.insert(name.to_owned(), f.into());
    }

    /// Removes a function from the runtime.
    ///
    /// Returns the function that was removed if it was found.
    pub fn deregister_function(&mut self, name: &str) -> Option<Arc<dyn Function>> {
        self.functions.remove(name)
    }

    /// Gets a function by name from the runtime.
    #[inline]
    pub fn get_function<'a>(&'a self, name: &str) -> Option<&'a dyn Function> {
        self.functions.get(name).map(AsRef::as_ref)
    }

    /// Registers all of the builtin JMESPath functions with the runtime.
    pub fn register_builtin_functions(&mut self) {
        self.register_function("abs", Arc::new(AbsFn::new()));
        self.register_function("avg", Arc::new(AvgFn::new()));
        self.register_function("ceil", Arc::new(CeilFn::new()));
        self.register_function("contains", Arc::new(ContainsFn::new()));
        self.register_function("ends_with", Arc::new(EndsWithFn::new()));
        self.register_function("floor", Arc::new(FloorFn::new()));
        self.register_function("join", Arc::new(JoinFn::new()));
        self.register_function("keys", Arc::new(KeysFn::new()));
        self.register_function("length", Arc::new(LengthFn::new()));
        self.register_function("map", Arc::new(MapFn::new()));
        self.register_function("min", Arc::new(MinFn::new()));
        self.register_function("max", Arc::new(MaxFn::new()));
        self.register_function("max_by", Arc::new(MaxByFn::new()));
        self.register_function("min_by", Arc::new(MinByFn::new()));
        self.register_function("merge", Arc::new(MergeFn::new()));
        self.register_function("not_null", Arc::new(NotNullFn::new()));
        self.register_function("reverse", Arc::new(ReverseFn::new()));
        self.register_function("sort", Arc::new(SortFn::new()));
        self.register_function("sort_by", Arc::new(SortByFn::new()));
        self.register_function("starts_with", Arc::new(StartsWithFn::new()));
        self.register_function("sum", Arc::new(SumFn::new()));
        self.register_function("to_array", Arc::new(ToArrayFn::new()));
        self.register_function("to_number", Arc::new(ToNumberFn::new()));
        self.register_function("to_string", Arc::new(ToStringFn::new()));
        self.register_function("type", Arc::new(TypeFn::new()));
        self.register_function("values", Arc::new(ValuesFn::new()));
    }
}
