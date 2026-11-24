//! Making sqlparse's `Token`/`TokenList` objects and filling their slots without going through Python calls.
//!
//! sqlparse's `Token` keeps its attributes in `__slots__` (`TokenList` adds `tokens`), and no subclass overrides
//! them, so each attribute lives at a fixed offset inside every such object. `setattr` finds that offset through
//! the class's member descriptor on each call; here the offsets are found once and the references are written
//! there directly, as the member descriptor would write them. Likewise `object.__new__(cls)` is called as the
//! C function behind it (`object`'s `tp_new`), without building an argument tuple and a method call each time.
//! A tree is a million such steps, so this is most of what making sqlparse's objects costs.
//!
//! The offsets are found on a probe object: each slot is set to a fresh object through `setattr`, and its offset
//! is where the object's memory holds that pointer (exactly one place, inside `__basicsize__`). Then an object
//! made and filled here is read back through `getattr` and must give every value back. The package hands over
//! only classes whose slots are `Token`'s and `TokenList`'s own and whose `__new__` is `object.__new__`
//! ([`Layout::find`] checks that too). Anything unexpected leaves the layout unknown, and the objects are made
//! with `object.__new__` and filled with `setattr`, as before.

use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyList, PyString, PyTuple, PyType};

/// The slots in the order [`Layout`] keeps their offsets.
pub const NAMES: [&str; 9] = [
    "value",
    "ttype",
    "parent",
    "is_group",
    "is_keyword",
    "is_whitespace",
    "is_newline",
    "normalized",
    "tokens",
];

/// Where each of [`NAMES`] lives in a `TokenList` (the first eight in a `Token` too: a subclass keeps its
/// bases' layout), and `object`'s `tp_new`.
pub struct Layout {
    offsets: [usize; 9],
    new: ffi::newfunc,
    empty: Py<PyTuple>,
}

/// What a node's slots are set to (`tokens` for a group only).
pub struct Slots<'a, 'py> {
    pub value: &'a Bound<'py, PyString>,
    pub ttype: &'a Bound<'py, PyAny>,
    pub parent: &'a Bound<'py, PyAny>,
    pub is_group: bool,
    pub is_keyword: bool,
    pub is_whitespace: bool,
    pub is_newline: bool,
    pub normalized: &'a Bound<'py, PyString>,
    pub tokens: Option<Bound<'py, PyList>>,
}

/// The class in `cls`'s MRO whose own namespace defines `name`, if any.
fn owner<'py>(cls: &Bound<'py, PyAny>, name: &str) -> Option<Bound<'py, PyAny>> {
    for klass in cls.getattr("__mro__").ok()?.try_iter().ok()? {
        let klass = klass.ok()?;
        if klass.getattr("__dict__").ok()?.contains(name).ok()? {
            return Some(klass);
        }
    }
    None
}

impl Layout {
    /// The layout of sqlparse's classes (`classes`, by name, `Token` and `TokenList` among them), checked as
    /// described above; None if anything is not as expected.
    pub fn find(py: Python<'_>, classes: &Bound<'_, PyDict>) -> Option<Layout> {
        let token = classes.get_item("Token").ok()??;
        let token_list = classes.get_item("TokenList").ok()??;
        let object_new = py
            .import("builtins")
            .ok()?
            .getattr("object")
            .ok()?
            .getattr("__new__")
            .ok()?;
        // every class: a type, made by object.__new__, its slots Token's and TokenList's own
        for (_, cls) in classes.iter() {
            if !cls.is_instance_of::<PyType>() || !cls.getattr("__new__").ok()?.is(&object_new) {
                return None;
            }
            let is_list = cls.cast::<PyType>().ok()?.is_subclass(&token_list).ok()?;
            for (i, name) in NAMES.iter().enumerate() {
                let want = if i < 8 { &token } else { &token_list };
                match owner(&cls, name) {
                    Some(o) if o.is(want) => {}
                    None if i == 8 && !is_list => {}
                    _ => return None,
                }
            }
        }
        // object's tp_new, which object.__new__(cls) calls after checking cls is a type it may make
        // SAFETY: PyBaseObject_Type is a static type object; PyType_GetSlot on it is fine from Python 3.10, which
        // the package requires.
        let slot = unsafe { ffi::PyType_GetSlot(&raw mut ffi::PyBaseObject_Type, ffi::Py_tp_new) };
        if slot.is_null() {
            return None;
        }
        // SAFETY: the tp_new slot holds a newfunc
        let new: ffi::newfunc = unsafe { std::mem::transmute(slot) };
        let ptr = std::mem::size_of::<*mut ffi::PyObject>();
        let header = std::mem::size_of::<ffi::PyObject>();
        let size_of = |cls: &Bound<'_, PyAny>| -> Option<usize> {
            cls.getattr("__basicsize__").ok()?.extract::<usize>().ok()
        };
        let list_size = size_of(&token_list)?;
        let token_size = size_of(&token)?;
        let mut layout = Layout {
            offsets: [0; 9],
            new,
            empty: PyTuple::empty(py).unbind(),
        };
        let fresh = || -> Option<Bound<'_, PyAny>> {
            py.import("builtins")
                .ok()?
                .getattr("object")
                .ok()?
                .call0()
                .ok()
        };
        let probe = layout.make(&token_list).ok()?;
        if !probe.get_type().is(&token_list) {
            return None;
        }
        for (i, name) in NAMES.iter().enumerate() {
            let marker = fresh()?;
            probe.setattr(*name, &marker).ok()?;
            let base = probe.as_ptr() as *const u8;
            let mut found = None;
            let mut off = header;
            while off + ptr <= list_size {
                // SAFETY: inside the object's own memory (`__basicsize__` bytes), at pointer alignment
                let word = unsafe { base.add(off).cast::<*mut ffi::PyObject>().read() };
                if word == marker.as_ptr() {
                    if found.is_some() {
                        return None;
                    }
                    found = Some(off);
                }
                off += ptr;
            }
            layout.offsets[i] = found?;
        }
        // distinct, and the Token's eight inside a Token
        for (i, a) in layout.offsets.iter().enumerate() {
            if layout.offsets[i + 1..].contains(a) || (i < 8 && a + ptr > token_size) {
                return None;
            }
        }
        // one object of every class made and filled here, read back through getattr
        for (_, cls) in classes.iter() {
            let is_list = cls.cast::<PyType>().ok()?.is_subclass(&token_list).ok()?;
            let obj = layout.make(&cls).ok()?;
            if !obj.get_type().is(&cls) {
                return None;
            }
            let value = PyString::new(py, "probe value");
            let normalized = PyString::new(py, "PROBE");
            let ttype = fresh()?;
            let parent = fresh()?;
            let tokens = PyList::empty(py);
            layout.fill(
                &obj,
                Slots {
                    value: &value,
                    ttype: &ttype,
                    parent: &parent,
                    is_group: true,
                    is_keyword: false,
                    is_whitespace: true,
                    is_newline: false,
                    normalized: &normalized,
                    tokens: is_list.then(|| tokens.clone()),
                },
            );
            let yes = PyBool::new(py, true).to_owned().into_any();
            let no = PyBool::new(py, false).to_owned().into_any();
            let expect: [&Bound<'_, PyAny>; 9] = [
                value.as_any(),
                &ttype,
                &parent,
                &yes,
                &no,
                &yes,
                &no,
                normalized.as_any(),
                tokens.as_any(),
            ];
            for (name, want) in NAMES.iter().zip(expect).take(if is_list { 9 } else { 8 }) {
                if !obj.getattr(*name).ok()?.is(want) {
                    return None;
                }
            }
        }
        Some(layout)
    }

    /// `object.__new__(cls)` for one of the classes [`Layout::find`] was given.
    pub fn make<'py>(&self, cls: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let py = cls.py();
        // SAFETY: `cls` is a type (checked in `find`) whose __new__ is object.__new__, which calls exactly this
        // with an empty argument tuple; the result is a new reference or NULL with an exception set.
        unsafe {
            let obj = (self.new)(
                cls.as_ptr().cast::<ffi::PyTypeObject>(),
                self.empty.as_ptr(),
                std::ptr::null_mut(),
            );
            Bound::from_owned_ptr_or_err(py, obj)
        }
    }

    /// What slot `i` of [`NAMES`] holds in `obj`, an instance of one of the classes `find` was given (a
    /// `TokenList` subclass for `tokens`): as the member descriptor reads it, None where it is unset (Python
    /// raises AttributeError there).
    pub fn read<'py>(&self, obj: &Bound<'py, PyAny>, i: usize) -> Option<Bound<'py, PyAny>> {
        // SAFETY: `obj` is an instance of a class with this layout (the caller checks its type), so the offset is
        // that slot's place in it; the reference read there is borrowed while the GIL is held, and taken here.
        unsafe {
            let word = obj
                .as_ptr()
                .cast::<u8>()
                .add(self.offsets[i])
                .cast::<*mut ffi::PyObject>()
                .read();
            Bound::from_borrowed_ptr_or_opt(obj.py(), word)
        }
    }

    /// Sets the slots of `obj`, an object [`Layout::make`] made of one of the classes `find` was given (a
    /// `TokenList` subclass when `slots.tokens` is given).
    pub fn fill(&self, obj: &Bound<'_, PyAny>, slots: Slots<'_, '_>) {
        let py = obj.py();
        let put = |i: usize, value: *mut ffi::PyObject| {
            // SAFETY: `obj` is an instance of a class with this layout (checked in `find`), so the offset is that
            // slot's place in it. The reference is owned; whatever the slot held is released, as the member
            // descriptor does.
            unsafe {
                let place = obj
                    .as_ptr()
                    .cast::<u8>()
                    .add(self.offsets[i])
                    .cast::<*mut ffi::PyObject>();
                let old = place.read();
                place.write(value);
                ffi::Py_XDECREF(old);
            }
        };
        let owned = |o: &Bound<'_, PyAny>| o.clone().into_ptr();
        let flag = |b: bool| PyBool::new(py, b).to_owned().into_any().into_ptr();
        put(0, owned(slots.value.as_any()));
        put(1, owned(slots.ttype));
        put(2, owned(slots.parent));
        put(3, flag(slots.is_group));
        put(4, flag(slots.is_keyword));
        put(5, flag(slots.is_whitespace));
        put(6, flag(slots.is_newline));
        put(7, owned(slots.normalized.as_any()));
        if let Some(tokens) = slots.tokens {
            put(8, tokens.into_any().into_ptr());
        }
    }
}
