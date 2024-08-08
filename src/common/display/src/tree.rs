use std::{
    fmt::{self, Write},
    sync::Arc,
};

use crate::{DisplayFormatType, Displayable};

pub trait TreeDisplay {
    // Required method: Get a list of lines representing this node. No trailing newlines.
    fn get_multiline_representation(&self) -> Vec<String>;

    // Required method: Get the human-readable name of this node.
    fn get_name(&self) -> String;

    // Required method: Get the children of the self node.
    fn get_children(&self) -> Vec<Arc<Self>>;

    // Print the whole tree represented by this node.
    fn fmt_tree(&self, s: &mut String, simple: bool) -> fmt::Result {
        self.fmt_tree_gitstyle(0, s, simple)
    }

    // Print the tree recursively, and illustrate the tree structure with a single line per node + indentation.
    fn fmt_tree_indent_style(&self, indent: usize, s: &mut String) -> fmt::Result {
        // Print the current node.
        if indent > 0 {
            writeln!(s)?;
            write!(s, "{:indent$}", "", indent = 2 * indent)?;
        }
        let node_str = self.get_multiline_representation().join(", ");
        write!(s, "{node_str}")?;

        // Recursively handle children.
        let children = self.get_children();
        match &children[..] {
            // No children - stop printing.
            [] => Ok(()),
            // One child.
            [child] => {
                // Child tree.
                child.fmt_tree_indent_style(indent + 1, s)
            }
            // Two children.
            [left, right] => {
                left.fmt_tree_indent_style(indent + 1, s)?;
                right.fmt_tree_indent_style(indent + 1, s)
            }
            _ => unreachable!("Max two child nodes expected, got {}", children.len()),
        }
    }

    // Print the tree recursively, and illustrate the tree structure in the same style as `git log --graph`.
    // `depth` is the number of forks in this node's ancestors.
    fn fmt_tree_gitstyle(&self, depth: usize, s: &mut String, simple: bool) -> fmt::Result {
        // Print the current node.
        // e.g. | | * <node contents line 1>
        //      | | | <node contents line 2>
        let lines = if simple {
            vec![self.get_name()]
        } else {
            self.get_multiline_representation()
        };
        use terminal_size::{terminal_size, Width};
        let size = terminal_size();
        let term_width = if let Some((Width(w), _)) = size {
            w as usize
        } else {
            88usize
        };

        let mut counter = 0;
        for val in lines.iter() {
            let base_characters = depth * 2;
            let expected_chars = (term_width - base_characters - 8).max(8);
            let sublines = textwrap::wrap(val, expected_chars);

            for (i, sb) in sublines.iter().enumerate() {
                self.fmt_depth(depth, s)?;
                match counter {
                    0 => write!(s, "* ")?,
                    _ => write!(s, "|   ")?,
                }
                counter += 1;
                match i {
                    0 => writeln!(s, "{sb}")?,
                    _ => writeln!(s, "  {sb}")?,
                }
            }
        }

        // Recursively handle children.
        let children = self.get_children();
        match &children[..] {
            // No children - stop printing.
            [] => Ok(()),
            // One child - print leg, then print the child tree.
            [child] => {
                // Leg: e.g. | | |
                self.fmt_depth(depth, s)?;
                writeln!(s, "|")?;

                // Child tree.
                child.fmt_tree_gitstyle(depth, s, simple)
            }
            // Two children - print legs, print right child indented, print left child.
            [left, right] => {
                // Legs: e.g. | | |\
                self.fmt_depth(depth, s)?;
                writeln!(s, "|\\")?;

                // Right child tree, indented.
                right.fmt_tree_gitstyle(depth + 1, s, simple)?;

                // Legs, e.g. | | |
                self.fmt_depth(depth, s)?;
                writeln!(s, "|")?;

                // Left child tree.
                left.fmt_tree_gitstyle(depth, s, simple)
            }
            _ => unreachable!("Max two child nodes expected, got {}", children.len()),
        }
    }

    fn fmt_depth(&self, depth: usize, s: &mut String) -> fmt::Result {
        // Print leading pipes for forks in ancestors that will be printed later.
        // e.g. "| | "
        for _ in 0..depth {
            write!(s, "| ")?;
        }
        Ok(())
    }
}

pub struct TreeDisplayVisitor<'a, W> {
    output: &'a mut W,
    t: DisplayFormatType,
}

impl<'a, W> TreeDisplayVisitor<'a, W> {
    pub fn new(w: &'a mut W, t: DisplayFormatType) -> Self {
        Self { output: w, t }
    }
}

impl<'a, W> TreeDisplayVisitor<'a, W>
where
    W: fmt::Write,
{
    // Print the tree recursively, and illustrate the tree structure with a single line per node + indentation.
    fn fmt_tree_indent_style<D: Displayable>(&mut self, node: &D, indent: usize) -> fmt::Result {
        // Print the current node.
        if indent > 0 {
            writeln!(self.output)?;
            write!(self.output, "{:indent$}", "", indent = 2 * indent)?;
        }
        let node_str = node.to_multiline_display(self.t)?.join(", ");
        write!(self.output, "{node_str}")?;

        // Recursively handle children.
        let children = node.parts(self.t);
        match &children[..] {
            // No children - stop printing.
            [] => Ok(()),
            // One child.
            [child] => {
                // Child tree.
                self.fmt_tree_indent_style(child, indent + 1)
            }
            // Two children.
            [left, right] => {
                self.fmt_tree_indent_style(left, indent + 1)?;
                self.fmt_tree_indent_style(right, indent + 1)
            }
            _ => unreachable!("Max two child nodes expected, got {}", children.len()),
        }
    }

    fn fmt_tree_gitstyle<D: Displayable + std::fmt::Debug>(
        &mut self,
        node: &D,
        depth: usize,
    ) -> fmt::Result {
        let val = node.to_string(self.t);
        use terminal_size::{terminal_size, Width};
        let size = terminal_size();
        let term_width = if let Some((Width(w), _)) = size {
            w as usize
        } else {
            88usize
        };

        let base_characters = depth * 2;
        let expected_chars = (term_width - base_characters - 8).max(8);
        let sublines = textwrap::wrap(&val, expected_chars);
        let mut counter = 0;

        for (i, sb) in sublines.iter().enumerate() {
            self.fmt_depth(depth)?;
            match counter {
                0 => write!(self.output, "* ")?,
                _ => write!(self.output, "|   ")?,
            }
            counter += 1;
            match i {
                0 => writeln!(self.output, "{sb}")?,
                _ => writeln!(self.output, "  {sb}")?,
            }
        }
        let children = node.parts(self.t);
        match &children[..] {
            [] => {}
            [child] => {
                self.fmt_depth(depth)?;

                writeln!(self.output, "|")?;

                // Child tree.
                self.fmt_tree_gitstyle(child, depth)?;
            }
            [left, right] => {
                // Legs: e.g. | | |\
                self.fmt_depth(depth)?;
                writeln!(self.output, "|\\")?;
                self.fmt_tree_gitstyle(right, depth + 1)?;

                // Legs, e.g. | | |
                self.fmt_depth(depth)?;
                writeln!(self.output, "|")?;

                // Left child tree.
                self.fmt_tree_gitstyle(left, depth)?;
            }
            _ => unreachable!("Max two child nodes expected, got {}", children.len()),
        }

        Ok(())
    }
    fn fmt_depth(&mut self, depth: usize) -> fmt::Result {
        // Print leading pipes for forks in ancestors that will be printed later.
        // e.g. "| | "
        for _ in 0..depth {
            write!(self.output, "| ")?;
        }
        Ok(())
    }

    // Print the whole tree represented by this node.
    pub fn fmt_tree<D: Displayable + std::fmt::Debug>(&mut self, node: &D) -> fmt::Result {
        self.fmt_tree_gitstyle(node, 0)
    }
}
