/// Focus index for a fixed set of form fields, independent of field semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormFocus {
    fields: usize,
    index: usize,
}

impl FormFocus {
    pub fn new(fields: usize) -> Self {
        assert!(fields > 0, "a form must contain at least one field");
        Self { fields, index: 0 }
    }

    pub fn index(self) -> usize {
        self.index
    }

    pub fn next(&mut self) {
        self.index = (self.index + 1) % self.fields;
    }

    pub fn reset(&mut self) {
        self.index = 0;
    }
}

impl Default for FormFocus {
    fn default() -> Self {
        Self::new(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_wraps_without_knowing_field_meaning() {
        let mut focus = FormFocus::new(2);
        assert_eq!(focus.index(), 0);
        focus.next();
        assert_eq!(focus.index(), 1);
        focus.next();
        assert_eq!(focus.index(), 0);
        focus.next();
        assert_eq!(focus.index(), 1);
        focus.reset();
        assert_eq!(focus.index(), 0);
    }
}
