use std::{borrow::Cow, fmt};

use crate::{exceptions::ExceptionRaise, expressions::FrameExit, heap::Heap, object::Object};

#[derive(Debug)]
pub enum Exit<'c, 'h> {
    Return(ReturnObject<'h>),
    // Yield(ReturnObject<'h>),
    Raise(ExceptionRaise<'c>),
}

impl fmt::Display for Exit<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Return(v) => write!(f, "{v}"),
            Self::Raise(exc) => write!(f, "{exc}"),
        }
    }
}

impl<'c, 'h> Exit<'c, 'h> {
    pub(crate) fn new(frame_exit: FrameExit<'c>, heap: &'h Heap) -> Self {
        match frame_exit {
            FrameExit::Return(object) => Self::Return(ReturnObject { object, heap }),
            FrameExit::Raise(exc) => Self::Raise(exc),
        }
    }
}

#[derive(Debug)]
pub struct ReturnObject<'h> {
    object: Object,
    heap: &'h Heap,
}

impl fmt::Display for ReturnObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.str())
    }
}

impl<'h> ReturnObject<'h> {
    /// User facing representation of the object, should match python's `str(object)`
    #[must_use]
    pub fn str(&self) -> Cow<'h, str> {
        self.object.str(self.heap)
    }

    /// Debug representation of the object, should match python's `repr(object)`
    #[must_use]
    pub fn repr(&self) -> Cow<'h, str> {
        self.object.repr(self.heap)
    }

    /// User facing representation of the object type, should roughly match `str(type(object))
    #[must_use]
    pub fn type_str(&self) -> &'static str {
        self.object.type_str(self.heap)
    }
}
