use std::{fmt, sync::Arc};

use mermaid::MermaidDisplayOptions;

pub mod mermaid;
pub mod tree;

#[derive(Debug, Clone, Copy)]
pub enum DisplayFormatType {
    /// Compact, showing only the
    Compact,
    /// Default, showing common details
    Default,
    /// Verbose, showing all available details
    Verbose,
}

pub struct Part<'a> {
    pub category: Option<&'static str>,
    pub value: DynDisplayable<'a>,
}

pub enum DynDisplayable<'a> {
    Borrowed(&'a dyn Displayable),
    Owned(Box<dyn Displayable>),
}

impl<'a> DynDisplayable<'a> {
    pub fn to_display(&'a self, t: DisplayFormatType) -> Result<String, fmt::Error> {
        let mut s = String::new();
        match self {
            DynDisplayable::Borrowed(b) => {
                b.fmt_self(t, &mut s)?;
            }
            DynDisplayable::Owned(o) => {
                o.fmt_self(t, &mut s)?;
            }
        };
        Ok(s)
    }
    pub fn parts(&'a self, t: DisplayFormatType) -> Vec<Part<'a>> {
        match self {
            DynDisplayable::Borrowed(b) => b.parts(t),
            DynDisplayable::Owned(o) => o.parts(t),
        }
    }
    pub fn to_multiline_display(&'a self, t: DisplayFormatType) -> Result<Vec<String>, fmt::Error> {
        match self {
            DynDisplayable::Borrowed(b) => b.to_multiline_display(t),
            DynDisplayable::Owned(o) => o.to_multiline_display(t),
        }
    }
    pub fn to_string(&'a self, t: DisplayFormatType) -> Result<String, fmt::Error> {
        let mut s = String::new();
        match self {
            DynDisplayable::Borrowed(b) => {
                b.fmt_self(t, &mut s)?;
            }
            DynDisplayable::Owned(o) => {
                o.fmt_self(t, &mut s)?;
            }
        };
        Ok(s)
    }
}

impl<'a> From<&'a dyn Displayable> for DynDisplayable<'a> {
    fn from(b: &'a dyn Displayable) -> Self {
        DynDisplayable::Borrowed(b)
    }
}

impl<'a> From<&'a dyn Displayable> for Part<'a> {
    fn from(b: &'a dyn Displayable) -> Self {
        Part {
            category: None,
            value: DynDisplayable::Borrowed(b),
        }
    }
}

impl<'a> Part<'a> {
    pub fn new_value<D: Displayable + 'static>(value: D) -> Self {
        Self {
            category: None,
            value: DynDisplayable::Owned(Box::new(value)),
        }
    }
    /// Create a new part with a borrowed value
    pub fn borrowed(category: &'static str, value: &'a dyn Displayable) -> Self {
        Self {
            category: Some(category),
            value: DynDisplayable::Borrowed(value),
        }
    }
    /// Create a new part with an owned value
    pub fn owned<D: Displayable + 'static>(category: &'static str, value: D) -> Self {
        Self {
            category: Some(category),
            value: DynDisplayable::Owned(Box::new(value)),
        }
    }

    /// Get nested parts from the value
    pub fn parts(&'a self, t: DisplayFormatType) -> Vec<Part<'a>> {
        self.value.parts(t)
    }

    /// Returns a vector of strings, where the first string is the main display string, and the rest are the parts.
    /// This **does not** recursively include nested parts
    pub fn to_multiline_display(&'a self, t: DisplayFormatType) -> Result<Vec<String>, fmt::Error> {
        let mut v = Vec::new();
        if let Some(category) = self.category {
            v.push(category.to_string());
        }
        v.extend(self.value.to_multiline_display(t)?);

        Ok(v)
    }
}

impl std::fmt::Debug for Part<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(category) = self.category {
            write!(
                f,
                "{}: {}",
                category,
                self.value.to_display(DisplayFormatType::Verbose)?
            )
        } else {
            write!(f, "{}", self.value.to_display(DisplayFormatType::Verbose)?)
        }
    }
}

impl std::fmt::Display for Part<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(category) = self.category {
            write!(
                f,
                "{}: {}",
                category,
                self.value.to_display(DisplayFormatType::Default)?
            )
        } else {
            write!(f, "{}", self.value.to_display(DisplayFormatType::Default)?)
        }
    }
}

/// If implementing this trait for a struct, it is up to the implementor to decide how to display the struct.
/// Some structs may make more sense to have everything in `fmt_self`,
/// others may want to use `parts` to display the struct and subcomponents separately.
///
///
/// Generally, if a struct has subcomponents that are also `Displayable`,
/// it is best to use `parts` to display the subcomponents.
///
/// # Example
/// ```rs,no_run
/// struct Config {
///     name: String,
///     bearer_token: Option<String>,
/// }
///
/// // This item has no `parts` and only displays itself
/// impl Displayable for Config {
///     fn fmt_self(&self, t: DisplayFormatType, f: &mut dyn fmt::Write) -> fmt::Result {
///         write!(f, "Config: {}", self.name)?;
///         if let Some(token) = &self.bearer_token {
///             writeln!(f, "Bearer: {}", token)?;
///         }
///         Ok(())
///     }
/// }
///
/// struct Task {
///     config: Config,
///     size_bytes_on_disk: Option<u64>,
///     subtasks: Vec<Task>,
/// }
///
/// impl Displayable for Task {
///     fn fmt_self(&self, t: DisplayFormatType, f: &mut dyn fmt::Write) -> fmt::Result {
///         f.write_str("Task")
///     }
///
///     fn parts<'a>(&'a self, t: DisplayFormatType) -> Vec<Part<'a>> {
///         let mut parts = Vec::with_capacity(2 + self.subtasks.len());
///         if let Some(size) = &self.size_bytes_on_disk {
///             parts.push(Part::borrowed("estimated size", size));
///         }
///
///         // Only include subtasks and 'config' in verbose or default mode
///         if matches!(t, DisplayFormatType::Verbose | DisplayFormatType::Default) {
///             parts.push(Part::borrowed("config", &self.config));
///             parts.extend(
///                 self.subtasks
///                     .iter()
///                     .map(|subtask| Part::borrowed("subtask", subtask)),
///             );
///         }
///         parts
///     }
/// }
/// ```
pub trait Displayable {
    /// Format according to `DisplayFormatType`.
    /// This should only include the data of the type itself, not its children.
    /// For example, a `LogicalPlan` node should only include its own data, not its children.

    fn fmt_self(&self, t: DisplayFormatType, f: &mut dyn fmt::Write) -> fmt::Result;
    fn parts<'a>(&'a self, _t: DisplayFormatType) -> Vec<Part<'a>>;
    fn to_compact_string(&self) -> String {
        self.to_string(DisplayFormatType::Compact)
    }
    fn to_verbose_string(&self) -> String {
        self.to_string(DisplayFormatType::Verbose)
    }
    fn to_part<'a>(&'a self) -> Part<'a>
    where
        Self: Sized,
    {
        Part {
            category: None,
            value: DynDisplayable::Borrowed(self),
        }
    }

    fn into_part(self) -> Part<'static>
    where
        Self: Sized + 'static,
    {
        Part {
            category: None,
            value: DynDisplayable::Owned(Box::new(self)),
        }
    }

    fn to_string<'a>(&'a self, t: DisplayFormatType) -> String {
        let mut s = String::new();
        self.fmt_self(t, &mut s).unwrap();
        s
    }

    /// Returns a vector of strings, where the first string is the main display string, and the rest are the parts.
    /// This **does not** recursively include nested parts
    fn to_multiline_display(&self, t: DisplayFormatType) -> Result<Vec<String>, fmt::Error> {
        let mut v = vec![];
        v.push(self.to_string(t));

        let parts = self.parts(t);
        for part in parts {
            v.push(format!("{part}"));
        }

        Ok(v)
    }
}

macro_rules! impl_displayable_for_numeric {
    ($($t:ty),*) => {
        $(
            impl Displayable for $t {
                fn fmt_self(&self, _t: DisplayFormatType, f: &mut dyn fmt::Write) -> fmt::Result {
                    write!(f, "{}", self)
                }
                fn parts<'a>(&'a self, _t: DisplayFormatType) -> Vec<Part<'a>> {
                    vec![]
                }
            }
        )*
    };
}

impl_displayable_for_numeric!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
);

impl Displayable for &str {
    fn fmt_self(&self, _t: DisplayFormatType, f: &mut dyn fmt::Write) -> fmt::Result {
        f.write_str(self)
    }
    fn parts<'a>(&'a self, _t: DisplayFormatType) -> Vec<Part<'a>> {
        vec![]
    }
}

impl Displayable for String {
    fn fmt_self(&self, _t: DisplayFormatType, f: &mut dyn fmt::Write) -> fmt::Result {
        f.write_str(self)
    }
    fn parts<'a>(&'a self, _t: DisplayFormatType) -> Vec<Part<'a>> {
        vec![]
    }
}

impl<T> Displayable for Arc<T>
where
    T: Displayable,
{
    fn fmt_self(&self, t: DisplayFormatType, f: &mut dyn fmt::Write) -> fmt::Result {
        self.as_ref().fmt_self(t, f)
    }
    fn parts<'a>(&'a self, t: DisplayFormatType) -> Vec<Part<'a>> {
        self.as_ref().parts(t)
    }
}

impl<T> Displayable for Box<T>
where
    T: Displayable,
{
    fn fmt_self(&self, t: DisplayFormatType, f: &mut dyn fmt::Write) -> fmt::Result {
        self.as_ref().fmt_self(t, f)
    }
    fn parts<'a>(&'a self, t: DisplayFormatType) -> Vec<Part<'a>> {
        self.as_ref().parts(t)
    }
}

impl<T> Displayable for Vec<T>
where
    T: Displayable,
{
    fn fmt_self(&self, t: DisplayFormatType, f: &mut dyn fmt::Write) -> fmt::Result {
        if self.is_empty() {
            f.write_str("[]")?;
        } else {
            f.write_str("[")?;
            for (i, p) in self.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                p.fmt_self(t, f)?;
            }
            f.write_str("]")?;
        };

        Ok(())
    }

    fn parts<'a>(&'a self, t: DisplayFormatType) -> Vec<Part<'a>> {
        vec![]
    }
}
impl Displayable for DynDisplayable<'_> {
    fn fmt_self(&self, t: DisplayFormatType, f: &mut dyn fmt::Write) -> fmt::Result {
        match self {
            DynDisplayable::Borrowed(r) => r.fmt_self(t, f),
            DynDisplayable::Owned(o) => o.fmt_self(t, f),
        }
    }

    fn parts<'a>(&'a self, t: DisplayFormatType) -> Vec<Part<'a>> {
        match self {
            DynDisplayable::Borrowed(r) => r.parts(t),
            DynDisplayable::Owned(o) => o.parts(t),
        }
    }
}

impl Displayable for Part<'_> {
    fn fmt_self(&self, t: DisplayFormatType, f: &mut dyn fmt::Write) -> fmt::Result {
        write!(f, "{self}")
    }

    fn parts<'a>(&'a self, t: DisplayFormatType) -> Vec<Part<'a>> {
        Part::parts(self, t)
    }
}

impl std::fmt::Debug for dyn Displayable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_self(DisplayFormatType::Verbose, f)
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "python", derive(pyo3::FromPyObject))]
// do not change the order of the variants, pyo3 matches from top to bottom,
// If any variants have a union, it will always match the first one,
// So we need to order them from most specific to least specific.
pub enum DisplayFormat {
    // Display the tree in Mermaid format.
    Mermaid(MermaidDisplayOptions),
    // Display the tree in ASCII format.
    Ascii { simple: bool },
}
