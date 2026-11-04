//! Outline views (ADR 0049): a table whose first column is a tree.
//!
//! The app gives the tree as a function from a node to its children; the view keeps the
//! *flattened* visible rows ([`Tree`]) and feeds them to a [`Table`], so the rows stay
//! virtual and everything a table does (columns, sorting, selection, keys) carries over.
//! The table's body reads the rows' levels and states through the object-safe [`FlatTree`]
//! to draw the disclosure triangles and indent, and to expand and collapse on a click or a
//! key.

use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::marker::PhantomData;
use std::rc::Rc;

use masonry_core::kurbo::Rect;

use super::scroll::ScrollWidget;
use super::table::{Cell, Column, SortOrder, Table};
use crate::Cx;
use crate::element::{Element, Handle};
use crate::widgets::RowsWidget;

/// The deepest `expand_all` goes, so a tree that loops back on itself ends.
const MAX_DEPTH: usize = 64;

/// What the table needs to know about a visible row of an outline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RowInfo {
    /// Depth: 0 for the top level.
    pub(crate) level: usize,
    /// It has children (it shows a disclosure triangle).
    pub(crate) expandable: bool,
    /// Its children show.
    pub(crate) expanded: bool,
    /// Place among its siblings, from 1.
    pub(crate) position: usize,
    /// How many siblings it has, itself included.
    pub(crate) set_size: usize,
}

/// What an edit of the tree did to the rows, for the selection to follow.
#[derive(Debug)]
pub(crate) struct Change {
    /// The number of rows now.
    pub(crate) len: usize,
    /// Where each row asked about is now (`None`: it is no longer visible).
    pub(crate) kept: Vec<Option<usize>>,
    /// The row that was collapsed: a selection hidden by it moves to it.
    pub(crate) collapsed: Option<usize>,
}

/// The flattened tree as the table sees it (without the app's node type).
pub(crate) trait FlatTree: std::fmt::Debug {
    fn len(&self) -> usize;
    fn info(&self, row: usize) -> Option<RowInfo>;
    /// The row of `row`'s parent, if it has one.
    fn parent(&self, row: usize) -> Option<usize>;
    /// Expands or collapses `row`, with all its descendants if `recursive`. `keep` are
    /// rows to follow through the change. `None`: nothing changed.
    fn set_expanded(
        &mut self,
        row: usize,
        expanded: bool,
        recursive: bool,
        keep: &[usize],
    ) -> Option<Change>;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct Node<Id> {
    id: Id,
    level: usize,
    expandable: bool,
    position: usize,
    set_size: usize,
}

type Children<Id> = Box<dyn FnMut(Option<&Id>) -> Vec<Id>>;
type Expandable<Id> = Box<dyn FnMut(&Id) -> bool>;

/// The visible rows of an outline, and which nodes are expanded.
pub(crate) struct Tree<Id> {
    children: Children<Id>,
    expandable: Option<Expandable<Id>>,
    rows: Vec<Node<Id>>,
    expanded: HashSet<Id>,
    /// The selection as last sent to the app, so only changes are sent again.
    reported: Vec<Id>,
}

impl<Id> std::fmt::Debug for Tree<Id> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tree")
            .field("rows", &self.rows.len())
            .field("expanded", &self.expanded.len())
            .finish_non_exhaustive()
    }
}

impl<Id: Clone + Eq + Hash + 'static> Tree<Id> {
    fn new(children: Children<Id>, expandable: Option<Expandable<Id>>) -> Self {
        let mut tree = Self {
            children,
            expandable,
            rows: Vec::new(),
            expanded: HashSet::new(),
            reported: Vec::new(),
        };
        tree.rows = tree.flatten_from(None, 0);
        tree
    }

    /// Whether `id` has children: the app's answer, or whether it has any.
    fn is_expandable(&mut self, id: &Id) -> bool {
        match &mut self.expandable {
            Some(f) => f(id),
            None => !(self.children)(Some(id)).is_empty(),
        }
    }

    /// The rows of `parent`'s descendants that show: its children, and the children of
    /// those that are expanded, and so on.
    fn flatten_from(&mut self, parent: Option<&Id>, level: usize) -> Vec<Node<Id>> {
        let mut out = Vec::new();
        self.flatten_into(parent, level, &mut out);
        out
    }

    fn flatten_into(&mut self, parent: Option<&Id>, level: usize, out: &mut Vec<Node<Id>>) {
        let kids = (self.children)(parent);
        let set_size = kids.len();
        for (i, id) in kids.into_iter().enumerate() {
            let expandable = self.is_expandable(&id);
            out.push(Node {
                id: id.clone(),
                level,
                expandable,
                position: i + 1,
                set_size,
            });
            if expandable && self.expanded.contains(&id) {
                self.flatten_into(Some(&id), level + 1, out);
            }
        }
    }

    /// Marks `id`'s descendants expanded, to a depth.
    fn expand_below(&mut self, id: &Id, depth: usize) {
        if depth >= MAX_DEPTH {
            return;
        }
        for child in (self.children)(Some(id)) {
            if self.is_expandable(&child) {
                self.expanded.insert(child.clone());
                self.expand_below(&child, depth + 1);
            }
        }
    }

    /// Forgets that `id`'s descendants were expanded (only expanded ones have any to
    /// visit).
    fn collapse_below(&mut self, id: &Id, depth: usize) {
        if depth >= MAX_DEPTH {
            return;
        }
        for child in (self.children)(Some(id)) {
            if self.expanded.remove(&child) {
                self.collapse_below(&child, depth + 1);
            }
        }
    }

    /// How many rows after `row` are inside it (deeper).
    fn descendants(&self, row: usize) -> usize {
        let level = self.rows[row].level;
        self.rows[row + 1..]
            .iter()
            .take_while(|n| n.level > level)
            .count()
    }

    pub(crate) fn id_at(&self, row: usize) -> Option<&Id> {
        self.rows.get(row).map(|n| &n.id)
    }

    pub(crate) fn index_of(&self, id: &Id) -> Option<usize> {
        self.rows.iter().position(|n| &n.id == id)
    }

    /// Reads the app's tree again, keeping what's expanded; `keep` are rows to follow.
    pub(crate) fn reload(&mut self, keep: &[usize]) -> Change {
        let ids: Vec<Option<Id>> = keep
            .iter()
            .map(|&r| self.rows.get(r).map(|n| n.id.clone()))
            .collect();
        self.rows = self.flatten_from(None, 0);
        let at: HashMap<&Id, usize> = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, n)| (&n.id, i))
            .collect();
        let kept = ids
            .iter()
            .map(|id| id.as_ref().and_then(|id| at.get(id).copied()))
            .collect();
        Change {
            len: self.rows.len(),
            kept,
            collapsed: None,
        }
    }

    /// Makes `id` expanded or not, showing the change if it is visible; a hidden node
    /// only remembers, to show when its parents open.
    pub(crate) fn set_id_expanded(
        &mut self,
        id: &Id,
        expanded: bool,
        keep: &[usize],
    ) -> Option<Change> {
        match self.index_of(id) {
            Some(row) => FlatTree::set_expanded(self, row, expanded, false, keep),
            None => {
                if expanded {
                    self.expanded.insert(id.clone());
                } else {
                    self.expanded.remove(id);
                }
                None
            }
        }
    }

    pub(crate) fn reported(&self) -> &[Id] {
        &self.reported
    }

    pub(crate) fn set_reported(&mut self, ids: Vec<Id>) {
        self.reported = ids;
    }

    /// Expands `ids` (those with children) before the view is shown.
    fn expand_initially(&mut self, ids: impl IntoIterator<Item = Id>) {
        self.expanded.extend(ids);
        self.rows = self.flatten_from(None, 0);
    }
}

impl<Id: Clone + Eq + Hash + 'static> FlatTree for Tree<Id> {
    fn len(&self) -> usize {
        self.rows.len()
    }

    fn info(&self, row: usize) -> Option<RowInfo> {
        let node = self.rows.get(row)?;
        let expanded = self.rows.get(row + 1).is_some_and(|n| n.level > node.level);
        Some(RowInfo {
            level: node.level,
            expandable: node.expandable,
            expanded: node.expandable && expanded,
            position: node.position,
            set_size: node.set_size,
        })
    }

    fn parent(&self, row: usize) -> Option<usize> {
        let level = self.rows.get(row)?.level.checked_sub(1)?;
        self.rows[..row].iter().rposition(|n| n.level == level)
    }

    fn set_expanded(
        &mut self,
        row: usize,
        expanded: bool,
        recursive: bool,
        keep: &[usize],
    ) -> Option<Change> {
        let node = self.rows.get(row)?;
        if !node.expandable {
            return None;
        }
        let (id, level) = (node.id.clone(), node.level);
        let open = self.rows.get(row + 1).is_some_and(|n| n.level > level);
        if open == expanded && !recursive {
            return None;
        }
        let removed = self.descendants(row);
        let old_end = row + 1 + removed;
        // Rows inside the part that is rebuilt are followed by their ids.
        let inside: Vec<(usize, Id)> = keep
            .iter()
            .enumerate()
            .filter(|&(_, &r)| r > row && r < old_end)
            .map(|(i, &r)| (i, self.rows[r].id.clone()))
            .collect();
        if expanded {
            self.expanded.insert(id.clone());
            if recursive {
                self.expand_below(&id, 0);
            }
        } else {
            self.expanded.remove(&id);
            if recursive {
                self.collapse_below(&id, 0);
            }
        }
        let below = if expanded {
            self.flatten_from(Some(&id), level + 1)
        } else {
            Vec::new()
        };
        let inserted = below.len();
        let new_ids: HashMap<Id, usize> = inside
            .iter()
            .filter_map(|(_, id)| {
                below
                    .iter()
                    .position(|n| &n.id == id)
                    .map(|p| (id.clone(), row + 1 + p))
            })
            .collect();
        self.rows.splice(row + 1..old_end, below);
        let mut kept: Vec<Option<usize>> = keep
            .iter()
            .map(|&r| {
                if r <= row {
                    Some(r)
                } else if r >= old_end {
                    Some(r - removed + inserted)
                } else {
                    None
                }
            })
            .collect();
        for (i, id) in inside {
            kept[i] = new_ids.get(&id).copied();
        }
        Some(Change {
            len: self.rows.len(),
            kept,
            collapsed: (!expanded).then_some(row),
        })
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

pub(crate) type SharedTree = Rc<RefCell<dyn FlatTree>>;

type OnSelect<M, Id> = Box<dyn FnMut(&[Id]) -> M>;
type OnActivate<M, Id> = Box<dyn FnMut(&Id) -> M>;

/// A table whose first column is a tree: rows with disclosure triangles that expand and
/// collapse, indented by level. `Id` names a node (a path, a database key); it must be
/// stable, because what is expanded and selected is remembered by it.
///
/// `children(None)` gives the top-level nodes and `children(Some(id))` a node's children,
/// in order. A node is expandable if it has any (the view asks as rows come into the
/// tree), or as [`with_expandable`](Self::with_expandable) says, so a folder's contents
/// needn't be read until it is opened. `cell(id, column)` gives each cell; the first
/// column is indented, and the triangle is drawn before its content. Like a table's, these
/// closures can't see the model: they read shared data, and after it changes
/// [`Handle::reload`] reads the tree again, keeping what is expanded and selected.
///
/// Everything a [`Table`] does works the same (multiple selection, sorting, resizing,
/// type-select in the first column), with these additions: a click on a triangle expands or
/// collapses the row (Option-click, Alt-click off macOS, all its descendants too), Right
/// expands the selected row or moves to its first child, and Left collapses it or moves to
/// its parent (Option or Alt: all descendants). Expanding keeps the selection on the same
/// rows; collapsing a row that hides selected rows selects it instead. The tree must be
/// finite: expanding everything stops at 64 levels.
pub struct Outline<M, Id> {
    table: Table<M>,
    tree: Rc<RefCell<Tree<Id>>>,
    on_select: Option<OnSelect<M, Id>>,
    on_activate: Option<OnActivate<M, Id>>,
    _id: PhantomData<fn() -> Id>,
}

impl<M, Id> std::fmt::Debug for Outline<M, Id> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outline")
            .field("table", &self.table)
            .finish_non_exhaustive()
    }
}

impl<M: 'static, Id: Clone + Eq + Hash + 'static> Outline<M, Id> {
    /// An outline of `columns` over the tree `children` describes, its cells from
    /// `cell(id, column)`.
    pub fn new<C: Into<Cell<M>>>(
        columns: impl IntoIterator<Item = Column>,
        children: impl FnMut(Option<&Id>) -> Vec<Id> + 'static,
        cell: impl FnMut(&Id, usize) -> C + 'static,
    ) -> Self {
        Self::build(columns, Box::new(children), None, cell)
    }

    /// Like [`new`](Self::new), with `expandable(id)` telling whether a node has children,
    /// without reading them.
    pub fn with_expandable<C: Into<Cell<M>>>(
        columns: impl IntoIterator<Item = Column>,
        children: impl FnMut(Option<&Id>) -> Vec<Id> + 'static,
        expandable: impl FnMut(&Id) -> bool + 'static,
        cell: impl FnMut(&Id, usize) -> C + 'static,
    ) -> Self {
        Self::build(
            columns,
            Box::new(children),
            Some(Box::new(expandable)),
            cell,
        )
    }

    fn build<C: Into<Cell<M>>>(
        columns: impl IntoIterator<Item = Column>,
        children: Children<Id>,
        expandable: Option<Expandable<Id>>,
        mut cell: impl FnMut(&Id, usize) -> C + 'static,
    ) -> Self {
        let tree = Rc::new(RefCell::new(Tree::new(children, expandable)));
        let rows = Rc::clone(&tree);
        let len = tree.borrow().rows.len();
        let table = Table::new(columns, len, move |row, column| -> Cell<M> {
            match rows.borrow().id_at(row) {
                Some(id) => cell(id, column).into(),
                None => Cell::text(""),
            }
        });
        let shared: SharedTree = Rc::clone(&tree) as SharedTree;
        table.shared.borrow_mut().outline = Some(shared);
        Self {
            table,
            tree,
            on_select: None,
            on_activate: None,
            _id: PhantomData,
        }
    }

    /// Starts with the nodes `ids` expanded (those with children, anywhere in the tree;
    /// they show once their parents are open).
    #[must_use]
    pub fn expanded(self, ids: impl IntoIterator<Item = Id>) -> Self {
        self.tree.borrow_mut().expand_initially(ids);
        self
    }

    /// Starts with the visible nodes `ids` selected.
    #[must_use]
    pub fn selected(self, ids: impl IntoIterator<Item = Id>) -> Self {
        let (rows, ids) = {
            let tree = self.tree.borrow();
            let ids: Vec<Id> = ids.into_iter().collect();
            let rows: Vec<usize> = ids.iter().filter_map(|id| tree.index_of(id)).collect();
            (rows, ids)
        };
        let len = self.tree.borrow().rows.len();
        self.table.shared.borrow_mut().selection.set(rows, len);
        self.tree.borrow_mut().set_reported(ids);
        self
    }

    /// Lets the user select more than one row.
    #[must_use]
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.table = self.table.multiple(multiple);
        self
    }

    /// Stripes every other row.
    #[must_use]
    pub fn alternating_rows(mut self, alternating: bool) -> Self {
        self.table = self.table.alternating_rows(alternating);
        self
    }

    /// Starts with the sort indicator on `column`.
    #[must_use]
    pub fn sorted(mut self, column: usize, order: SortOrder) -> Self {
        self.table = self.table.sorted(column, order);
        self
    }

    /// Sends the message `make` returns, given the selected nodes (in row order), when the
    /// user changes the selection. Changes through a [`Handle`] send nothing, and neither
    /// does expanding when the same nodes stay selected.
    #[must_use]
    pub fn on_select(mut self, make: impl FnMut(&[Id]) -> M + 'static) -> Self {
        self.on_select = Some(Box::new(make));
        self
    }

    /// Sends the message `make` returns, given the node, when the user double-clicks a
    /// row or presses Return. Without it, Return goes on to the default button.
    #[must_use]
    pub fn on_activate(mut self, make: impl FnMut(&Id) -> M + 'static) -> Self {
        self.on_activate = Some(Box::new(make));
        self
    }

    /// Sends the message `make` returns, given the column and order, when the user clicks
    /// a sortable column's title. The app sorts the children it gives, then calls
    /// [`Handle::reload`].
    #[must_use]
    pub fn on_sort(mut self, make: impl FnMut(usize, SortOrder) -> M + 'static) -> Self {
        self.table = self.table.on_sort(make);
        self
    }

    /// The width it asks for, in logical pixels.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.table = self.table.width(width);
        self
    }

    /// The height it asks for, in logical pixels.
    #[must_use]
    pub fn height(mut self, height: f64) -> Self {
        self.table = self.table.height(height);
        self
    }

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.table = self.table.disabled(disabled);
        self
    }

    /// Points `handle` at this outline.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.table.scroll_id());
        self
    }
}

impl<M: 'static, Id: Clone + Eq + Hash + 'static> From<Outline<M, Id>> for Element<M> {
    fn from(outline: Outline<M, Id>) -> Self {
        let Outline {
            mut table,
            tree,
            on_select,
            on_activate,
            ..
        } = outline;
        // The table's own select and activate hooks speak in rows; the app's in nodes.
        let rows = Rc::clone(&tree);
        if let Some(mut f) = on_select {
            table = table.on_select_opt(move |selected| {
                let mut tree = rows.borrow_mut();
                let ids: Vec<Id> = selected
                    .iter()
                    .filter_map(|&r| tree.id_at(r).cloned())
                    .collect();
                // Expanding moves rows without changing what is selected.
                if ids == tree.reported() {
                    return None;
                }
                tree.set_reported(ids.clone());
                Some(f(&ids))
            });
        }
        let rows = Rc::clone(&tree);
        if let Some(mut f) = on_activate {
            table = table.on_activate_opt(move |row| {
                let tree = rows.borrow();
                tree.id_at(row).map(&mut f)
            });
        }
        table.into()
    }
}

impl<M, Id: Clone + Eq + Hash + 'static> Handle<Outline<M, Id>> {
    /// Reads the tree again, after the data behind `children` and `cell` changed. What is
    /// expanded and selected is kept (by node); nodes that are gone are dropped.
    pub fn reload<N>(&self, cx: &mut Cx<'_, N>) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, |rows| {
                RowsWidget::edit_tree(rows, |tree, keep| {
                    let tree = tree.as_any_mut().downcast_mut::<Tree<Id>>()?;
                    Some(tree.reload(keep))
                });
            });
        });
    }

    /// Expands or collapses the node `id` (not its descendants). A node whose parents are
    /// closed only remembers, and shows so when they open. Sends no message.
    pub fn set_expanded<N>(&self, cx: &mut Cx<'_, N>, id: &Id, expanded: bool) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, |rows| {
                RowsWidget::edit_tree(rows, |tree, keep| {
                    let tree = tree.as_any_mut().downcast_mut::<Tree<Id>>()?;
                    tree.set_id_expanded(id, expanded, keep)
                });
            });
        });
    }

    /// Selects the visible nodes `ids`. Sends no message.
    pub fn set_selection<N>(&self, cx: &mut Cx<'_, N>, ids: impl IntoIterator<Item = Id>) {
        let ids: Vec<Id> = ids.into_iter().collect();
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, |rows| {
                let rows_of = RowsWidget::with_tree::<Id, _>(rows, |tree| {
                    let selected: Vec<usize> =
                        ids.iter().filter_map(|id| tree.index_of(id)).collect();
                    let kept: Vec<Id> = selected
                        .iter()
                        .filter_map(|&r| tree.id_at(r).cloned())
                        .collect();
                    tree.set_reported(kept);
                    selected
                });
                if let Some(selected) = rows_of {
                    RowsWidget::set_selection(rows, selected);
                }
            });
        });
    }

    /// Scrolls as little as it takes to show the row of the visible node `id` whole.
    pub fn scroll_to<N>(&self, cx: &mut Cx<'_, N>, id: &Id) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            let span = ScrollWidget::with_rows(&mut w, |rows| {
                let row = RowsWidget::with_tree::<Id, _>(rows, |tree| tree.index_of(id))??;
                let (y0, y1) = rows.widget.row_span(row);
                let x = rows.widget.window.x0;
                Some(Rect::new(x, y0, x, y1))
            });
            if let Some(Some(target)) = span {
                ScrollWidget::edit_reveal(&mut w, target);
            }
        });
    }

    /// Enables or disables the outline.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<ScrollWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree of numbers: node `n` has children `10n+1 ..= 10n+3`, except that 2 has none
    /// and nothing past four digits does.
    fn kids(parent: Option<&u32>) -> Vec<u32> {
        match parent {
            None => vec![1, 2, 3],
            Some(&n) if n >= 1000 || n == 2 => vec![],
            Some(&n) => (1..=3).map(|i| n * 10 + i).collect(),
        }
    }

    fn tree() -> Tree<u32> {
        Tree::new(Box::new(kids), None)
    }

    fn ids(t: &Tree<u32>) -> Vec<u32> {
        t.rows.iter().map(|n| n.id).collect()
    }

    #[test]
    fn collapsed_trees_show_the_top_level_only() {
        let t = tree();
        assert_eq!(ids(&t), [1, 2, 3]);
        let one = t.info(0).unwrap();
        assert!(one.expandable && !one.expanded);
        assert_eq!((one.position, one.set_size, one.level), (1, 3, 0));
        assert!(!t.info(1).unwrap().expandable, "2 has no children");
    }

    #[test]
    fn expanding_inserts_the_children_and_collapsing_removes_them() {
        let mut t = tree();
        let c = t.set_expanded(0, true, false, &[2, 1]).unwrap();
        assert_eq!(ids(&t), [1, 11, 12, 13, 2, 3]);
        assert_eq!((c.len, c.kept.clone()), (6, vec![Some(5), Some(4)]));
        assert!(t.info(0).unwrap().expanded);
        assert_eq!(t.info(2).unwrap().level, 1);
        assert_eq!(t.parent(2), Some(0));
        assert_eq!(t.parent(0), None);
        assert!(
            t.set_expanded(0, true, false, &[]).is_none(),
            "already open"
        );
        let c = t.set_expanded(0, false, false, &[2, 5]).unwrap();
        assert_eq!(ids(&t), [1, 2, 3]);
        assert_eq!(
            c.kept,
            [None, Some(2)],
            "a hidden row is lost, a later one moves"
        );
        assert_eq!(c.collapsed, Some(0));
    }

    #[test]
    fn collapsing_remembers_what_was_open_below_unless_recursive() {
        let mut t = tree();
        t.set_expanded(0, true, false, &[]);
        t.set_expanded(1, true, false, &[]);
        assert_eq!(ids(&t), [1, 11, 111, 112, 113, 12, 13, 2, 3]);
        t.set_expanded(0, false, false, &[]);
        t.set_expanded(0, true, false, &[]);
        assert_eq!(ids(&t).len(), 9, "11 is still open");
        t.set_expanded(0, false, true, &[]);
        t.set_expanded(0, true, false, &[]);
        assert_eq!(ids(&t), [1, 11, 12, 13, 2, 3], "recursive collapse forgets");
    }

    #[test]
    fn recursive_expansion_opens_every_descendant_and_follows_rows_inside() {
        let mut t = tree();
        t.set_expanded(0, true, false, &[]);
        // Row 3 (13) is inside what is rebuilt: it moves by its id.
        let c = t.set_expanded(0, true, true, &[3, 4]).unwrap();
        assert_eq!(t.rows[c.kept[0].unwrap()].id, 13);
        assert_eq!(t.rows[c.kept[1].unwrap()].id, 2);
        assert!(t.rows.iter().any(|n| n.id == 1111), "opened to the bottom");
        assert!(t.rows.iter().all(|n| n.id < 10_000), "and stopped there");
    }

    #[test]
    fn reload_keeps_expansion_and_follows_rows_by_id() {
        let mut t = tree();
        t.set_expanded(1, false, false, &[]);
        t.set_expanded(0, true, false, &[]);
        let c = t.reload(&[3, 9]);
        assert_eq!(ids(&t), [1, 11, 12, 13, 2, 3]);
        assert_eq!(c.kept, [Some(3), None], "9 was never a row");
        assert_eq!(c.len, 6);
    }

    #[test]
    fn hidden_nodes_only_remember() {
        let mut t = tree();
        assert!(t.set_id_expanded(&11, true, &[]).is_none());
        assert_eq!(ids(&t), [1, 2, 3]);
        t.set_expanded(0, true, false, &[]);
        assert_eq!(ids(&t), [1, 11, 111, 112, 113, 12, 13, 2, 3]);
    }

    #[test]
    fn expandable_can_be_answered_without_reading_children() {
        let mut t = Tree::new(Box::new(kids), Some(Box::new(|&n: &u32| n == 3)));
        assert!(!t.info(0).unwrap().expandable);
        assert!(t.info(2).unwrap().expandable);
        t.set_expanded(2, true, false, &[]);
        // Children are read when opened; they aren't expandable by that answer.
        assert_eq!(ids(&t), [1, 2, 3, 31, 32, 33]);
    }
}
