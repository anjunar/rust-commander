use std::collections::BTreeSet;

#[derive(Clone, Debug, Default)]
pub struct SelectionModel {
    anchor_index: Option<usize>,
    focused_index: Option<usize>,
    selected_indices: BTreeSet<usize>,
}

impl SelectionModel {
    pub fn new(
        selected_indices: BTreeSet<usize>,
        focused_index: Option<usize>,
        anchor_index: Option<usize>,
    ) -> Self {
        Self {
            anchor_index,
            focused_index,
            selected_indices,
        }
    }

    pub fn single(index: Option<usize>) -> Self {
        match index {
            Some(index) => {
                let mut selected_indices = BTreeSet::new();
                selected_indices.insert(index);
                Self::new(selected_indices, Some(index), Some(index))
            }
            None => Self::default(),
        }
    }

    pub fn anchor_index(&self) -> Option<usize> {
        self.anchor_index
    }

    pub fn focus_index(&self) -> Option<usize> {
        self.focused_index
    }

    pub fn primary_index(&self) -> Option<usize> {
        self.focused_index
            .or_else(|| self.selected_indices.iter().next().copied())
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.selected_indices.contains(&index)
    }

    pub fn selected_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.selected_indices.iter().copied()
    }

    pub fn set_from_indices(&mut self, indices: impl IntoIterator<Item = usize>, len: usize) {
        let mut selected = indices
            .into_iter()
            .filter(|index| *index < len)
            .collect::<BTreeSet<_>>();
        let focused = selected.iter().next().copied();
        if selected.is_empty() && len > 0 {
            selected.insert(0);
        }

        self.anchor_index = focused.or_else(|| (len > 0).then_some(0));
        self.focused_index = self.anchor_index;
        self.selected_indices = selected;
    }

    pub fn set_single(&mut self, index: usize, len: usize) {
        if index < len {
            self.anchor_index = Some(index);
            self.focused_index = Some(index);
            self.selected_indices.clear();
            self.selected_indices.insert(index);
        }
    }

    pub fn clear(&mut self) {
        self.anchor_index = None;
        self.focused_index = None;
        self.selected_indices.clear();
    }
}
