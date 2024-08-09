pub struct Head {
    parent: Box<super::IFilter>,
    n: usize,
}
impl Head {
    pub fn new(parent: Box<super::IFilter>, n: usize) -> Self {
        Self { parent, n }
    }
}
impl super::Transform for Head {
    fn transform(&self, data: &mut [u8]) {
        let part = &mut data[..self.n];
        self.parent.transform(part.as_mut());
    }
}
#[cfg(test)]
mod test {
    use super::*;
    use crate::filters::Transform;

    struct Add1;
    impl Transform for Add1 {
        fn transform(&self, data: &mut [u8]) {
            data.iter_mut().for_each(|b| *b += 1);
        }
    }

    #[test]
    fn head0() {
        let add_filter = Add1;
        let head_filter = Head::new(Box::new(add_filter), 0);
        let mut data = [0, 0, 0, 0, 0];
        head_filter.transform(data.as_mut());
        assert_eq!(data, [0, 0, 0, 0, 0]);
    }

    #[test]
    fn head2() {
        let add_filter = Add1;
        let head_filter = Head::new(Box::new(add_filter), 2);
        let mut data = [99, 99, 0, 0, 0];
        head_filter.transform(data.as_mut());
        assert_eq!(data, [100, 100, 0, 0, 0]);
    }
}
