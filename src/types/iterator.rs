// Copyright (c) 2017-present PyO3 Project and Contributors
//
// based on Daniel Grunwald's https://github.com/dgrunwald/rust-cpython

use crate::{ffi, AsPyPointer, PyAny, PyErr, PyNativeType, PyResult, Python};

/// A Python iterator object.
///
/// Unlike other Python objects, this class includes a `Python<'p>` token
/// so that `PyIterator` can implement the Rust `Iterator` trait.
///
/// # Example
///
/// ```rust
/// # use pyo3::prelude::*;
/// use pyo3::types::PyIterator;
///
/// # fn main() -> PyResult<()> {
/// let gil = Python::acquire_gil();
/// let py = gil.python();
/// let list = py.eval("iter([1, 2, 3, 4])", None, None)?;
/// let numbers: PyResult<Vec<usize>> = list.iter()?.map(|i| i.and_then(PyAny::extract::<usize>)).collect();
/// let sum: usize = numbers?.iter().sum();
/// assert_eq!(sum, 10);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct PyIterator<'p>(&'p PyAny);

impl<'p> PyIterator<'p> {
    /// Constructs a `PyIterator` from a Python iterator object.
    pub fn from_object<T>(py: Python<'p>, obj: &T) -> PyResult<PyIterator<'p>>
    where
        T: AsPyPointer,
    {
        let iter = unsafe {
            // This looks suspicious, but is actually correct. Even though ptr is an owned
            // reference, PyIterator takes ownership of the reference and decreases the count
            // in its Drop implementation.
            //
            // Therefore we must use from_borrowed_ptr_or_err instead of from_owned_ptr_or_err so
            // that the GILPool does not take ownership of the reference.
            py.from_borrowed_ptr_or_err(ffi::PyObject_GetIter(obj.as_ptr()))?
        };

        Ok(PyIterator(iter))
    }
}

impl<'p> Iterator for PyIterator<'p> {
    type Item = PyResult<&'p PyAny>;

    /// Retrieves the next item from an iterator.
    ///
    /// Returns `None` when the iterator is exhausted.
    /// If an exception occurs, returns `Some(Err(..))`.
    /// Further `next()` calls after an exception occurs are likely
    /// to repeatedly result in the same exception.
    fn next(&mut self) -> Option<Self::Item> {
        let py = self.0.py();

        match unsafe { py.from_owned_ptr_or_opt(ffi::PyIter_Next(self.0.as_ptr())) } {
            Some(obj) => Some(Ok(obj)),
            None => {
                if PyErr::occurred(py) {
                    Some(Err(PyErr::fetch(py)))
                } else {
                    None
                }
            }
        }
    }
}

/// Dropping a `PyIterator` instance decrements the reference count on the object by 1.
impl<'p> Drop for PyIterator<'p> {
    fn drop(&mut self) {
        unsafe { ffi::Py_DECREF(self.0.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::PyIterator;
    use crate::exceptions::PyTypeError;
    use crate::gil::GILPool;
    use crate::types::{PyDict, PyList};
    use crate::Python;
    use crate::ToPyObject;
    use indoc::indoc;

    #[test]
    fn vec_iter() {
        let gil_guard = Python::acquire_gil();
        let py = gil_guard.python();
        let obj = vec![10, 20].to_object(py);
        let inst = obj.as_ref(py);
        let mut it = inst.iter().unwrap();
        assert_eq!(10, it.next().unwrap().unwrap().extract().unwrap());
        assert_eq!(20, it.next().unwrap().unwrap().extract().unwrap());
        assert!(it.next().is_none());
    }

    #[test]
    fn iter_refcnt() {
        let obj;
        let count;
        {
            let gil_guard = Python::acquire_gil();
            let py = gil_guard.python();
            obj = vec![10, 20].to_object(py);
            count = obj.get_refcnt(py);
        }

        {
            let gil_guard = Python::acquire_gil();
            let py = gil_guard.python();
            let inst = obj.as_ref(py);
            let mut it = inst.iter().unwrap();

            assert_eq!(10, it.next().unwrap().unwrap().extract().unwrap());
        }
        assert_eq!(count, obj.get_refcnt(Python::acquire_gil().python()));
    }

    #[test]
    fn iter_item_refcnt() {
        let gil_guard = Python::acquire_gil();
        let py = gil_guard.python();

        let obj;
        let none;
        let count;
        {
            let _pool = unsafe { GILPool::new() };
            let l = PyList::empty(py);
            none = py.None();
            l.append(10).unwrap();
            l.append(&none).unwrap();
            count = none.get_refcnt(py);
            obj = l.to_object(py);
        }

        {
            let _pool = unsafe { GILPool::new() };
            let inst = obj.as_ref(py);
            let mut it = inst.iter().unwrap();

            assert_eq!(10, it.next().unwrap().unwrap().extract().unwrap());
            assert!(it.next().unwrap().unwrap().is_none());
        }
        assert_eq!(count, none.get_refcnt(py));
    }

    #[test]
    fn fibonacci_generator() {
        let fibonacci_generator = indoc!(
            r#"
            def fibonacci(target):
                a = 1
                b = 1
                for _ in range(target):
                    yield a
                    a, b = b, a + b
        "#
        );

        let gil = Python::acquire_gil();
        let py = gil.python();

        let context = PyDict::new(py);
        py.run(fibonacci_generator, None, Some(context)).unwrap();

        let generator = py.eval("fibonacci(5)", None, Some(context)).unwrap();
        for (actual, expected) in generator.iter().unwrap().zip(&[1, 1, 2, 3, 5]) {
            let actual = actual.unwrap().extract::<usize>().unwrap();
            assert_eq!(actual, *expected)
        }
    }

    #[test]
    fn int_not_iterable() {
        let gil = Python::acquire_gil();
        let py = gil.python();

        let x = 5.to_object(py);
        let err = PyIterator::from_object(py, &x).unwrap_err();

        assert!(err.is_instance::<PyTypeError>(py))
    }
}
