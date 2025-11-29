use std::borrow::Cow;

use crate::exceptions::{check_arg_count, exc_err_fmt, ExcType};
use crate::heap::{Heap, HeapData, ObjectId};
use crate::object::{Attr, Object};
use crate::run::RunResult;
use crate::values::PyValue;

/// Python list type, wrapping a Vec of Objects.
///
/// This type provides Python list semantics including dynamic growth,
/// reference counting for heap objects, and standard list methods like
/// append and insert.
///
/// # Reference Counting
/// When objects are added to the list (via append, insert, etc.), their
/// reference counts are incremented if they are heap-allocated (Ref variants).
/// This ensures objects remain valid while referenced by the list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct List(Vec<Object>);

impl List {
    /// Creates a new list from a vector of objects.
    ///
    /// Note: This does NOT increment reference counts - the caller must
    /// ensure refcounts are properly managed.
    #[must_use]
    pub fn from_vec(vec: Vec<Object>) -> Self {
        Self(vec)
    }

    /// Returns a reference to the underlying vector.
    #[must_use]
    pub fn as_vec(&self) -> &Vec<Object> {
        &self.0
    }

    /// Returns a mutable reference to the underlying vector.
    ///
    /// # Safety Considerations
    /// Be careful when mutating the vector directly - you must manually
    /// manage reference counts for any heap objects you add or remove.
    pub fn as_vec_mut(&mut self) -> &mut Vec<Object> {
        &mut self.0
    }

    /// Returns the number of elements in the list.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Appends an element to the end of the list.
    ///
    /// If the item is a heap-allocated object (Ref variant), its reference
    /// count is incremented automatically.
    ///
    /// Returns `Object::None`, matching Python's behavior where `list.append()` returns None.
    pub fn append(&mut self, heap: &mut Heap, item: Object) -> Object {
        // Increment refcount if item is heap-allocated
        if let Object::Ref(item_id) = &item {
            heap.inc_ref(*item_id);
        }
        self.0.push(item);
        Object::None
    }

    /// Inserts an element at the specified index.
    ///
    /// If the item is a heap-allocated object (Ref variant), its reference
    /// count is incremented automatically.
    ///
    /// # Arguments
    /// * `index` - The position to insert at (0-based). If index >= len(),
    ///   the item is appended to the end (matching Python semantics).
    ///
    /// Returns `Object::None`, matching Python's behavior where `list.insert()` returns None.
    pub fn insert(&mut self, heap: &mut Heap, index: usize, item: Object) -> Object {
        // Increment refcount if item is heap-allocated
        if let Object::Ref(item_id) = &item {
            heap.inc_ref(*item_id);
        }

        // Python's insert() appends if index is out of bounds
        if index >= self.0.len() {
            self.0.push(item);
        } else {
            self.0.insert(index, item);
        }

        Object::None
    }
}

impl From<List> for Vec<Object> {
    fn from(list: List) -> Self {
        list.0
    }
}

impl PyValue for List {
    fn py_type(&self, _heap: &Heap) -> &'static str {
        "list"
    }

    fn py_len(&self, _heap: &Heap) -> Option<usize> {
        Some(self.0.len())
    }

    fn py_eq(&self, other: &Self, heap: &Heap) -> bool {
        self.0.len() == other.0.len() && self.0.iter().zip(&other.0).all(|(i1, i2)| i1.py_eq(i2, heap))
    }

    fn py_dec_ref_ids(&self, stack: &mut Vec<ObjectId>) {
        for obj in &self.0 {
            if let Object::Ref(id) = obj {
                stack.push(*id);
            }
        }
    }

    fn py_bool(&self, _heap: &Heap) -> bool {
        !self.0.is_empty()
    }

    fn py_repr<'h>(&'h self, heap: &'h Heap) -> Cow<'h, str> {
        Cow::Owned(repr_sequence('[', ']', &self.0, heap))
    }

    fn py_add(&self, other: &Self, heap: &mut Heap) -> Option<Object> {
        let mut result = self.0.clone();
        result.extend_from_slice(other.as_vec());
        for obj in &result {
            if let Object::Ref(id) = obj {
                heap.inc_ref(*id);
            }
        }
        let id = heap.allocate(HeapData::List(List::from_vec(result)));
        Some(Object::Ref(id))
    }

    fn py_iadd(&mut self, other: Object, heap: &mut Heap, self_id: Option<ObjectId>) -> Result<(), Object> {
        let rhs = match other {
            Object::Ref(other_id) => {
                if Some(other_id) == self_id {
                    self.0.clone()
                } else if let HeapData::List(list) = heap.get(other_id) {
                    list.as_vec().clone()
                } else {
                    return Err(Object::Ref(other_id));
                }
            }
            _ => return Err(other),
        };

        for obj in &rhs {
            if let Object::Ref(id) = obj {
                heap.inc_ref(*id);
            }
        }

        self.0.extend(rhs);
        Ok(())
    }

    fn py_call_attr<'c>(&mut self, heap: &mut Heap, attr: &Attr, args: Vec<Object>) -> RunResult<'c, Object> {
        match attr {
            Attr::Append => {
                let [item] = check_arg_count::<1>("list.append", args)?;
                Ok(self.append(heap, item))
            }
            Attr::Insert => {
                let [index_obj, item] = check_arg_count::<2>("list.insert", args)?;
                let index = index_obj.as_int()? as usize;
                Ok(self.insert(heap, index, item))
            }
            Attr::Get | Attr::Keys | Attr::Values | Attr::Items | Attr::Pop | Attr::Other(_) => {
                exc_err_fmt!(
                    ExcType::AttributeError;
                    "'list' object has no attribute '{}'",
                    attr
                )
            }
        }
    }
}

/// Formats a sequence of objects with the given start and end characters.
///
/// This helper function is used to implement `__repr__` for sequence types like
/// lists and tuples. It formats items as comma-separated repr strings.
///
/// # Arguments
/// * `start` - The opening character (e.g., '[' for lists, '(' for tuples)
/// * `end` - The closing character (e.g., ']' for lists, ')' for tuples)
/// * `items` - The slice of objects to format
/// * `heap` - The heap for resolving object references
///
/// # Returns
/// A string representation like "[1, 2, 3]" or "(1, 2, 3)"
pub(crate) fn repr_sequence(start: char, end: char, items: &[Object], heap: &Heap) -> String {
    let mut s = String::from(start);
    let mut iter = items.iter();
    if let Some(first) = iter.next() {
        let repr = first.py_repr(heap);
        s.push_str(repr.as_ref());
        for item in iter {
            s.push_str(", ");
            let repr = item.py_repr(heap);
            s.push_str(repr.as_ref());
        }
    }
    s.push(end);
    s
}
