pub struct Xor {
    key: Vec<u8>,
}

impl Xor {
    pub fn with_key(key: Vec<u8>) -> Self {
        Self { key }
    }
}

impl super::Transform for Xor {
    fn transform(&self, data: &mut [u8]) {
        for (plain_char, key_char) in data.iter_mut().zip(self.key.iter().cycle()) {
            *plain_char ^= key_char;
        }
    }
}

#[cfg(test)]
mod test {
    use crate::filters::Transform;

    use super::*;

    #[test]
    fn epmty_key_empty_message() {
        let xor_cipher = Xor::with_key(vec![]);
        let mut data = [];
        xor_cipher.transform(&mut data);
        assert_eq!(data, []);
    }

    #[test]
    fn epmty_key_nonempty_message() {
        let xor_cipher = Xor::with_key(vec![]);
        let mut data = [0, 1, 2, 3];
        xor_cipher.transform(&mut data);
        assert_eq!(data, [0, 1, 2, 3]);
    }

    #[test]
    fn nonepmty_key_empty_message() {
        let xor_cipher = Xor::with_key(vec![0, 1, 2, 3]);
        let mut data = [];
        xor_cipher.transform(&mut data);
        assert_eq!(data, []);
    }

    #[test]
    fn nonepmty_key_nonempty_message() {
        let xor_cipher = Xor::with_key(vec![0, 1, 2, 3]);
        let mut data = [0, 1, 2, 3];
        xor_cipher.transform(&mut data);
        assert_eq!(data, [0, 0, 0, 0]);
    }

    #[test]
    fn longer_key_shorter_message() {
        let xor_cipher = Xor::with_key(vec![1, 1, 1, 1, 1, 1, 1]);
        let mut data = [2, 2, 2];
        xor_cipher.transform(&mut data);
        assert_eq!(data, [3, 3, 3]);
    }

    #[test]
    fn shorter_key_longer_message() {
        let xor_cipher = Xor::with_key(vec![1, 1, 1]);
        let mut data = [2, 2, 2, 2, 2, 2];
        xor_cipher.transform(&mut data);
        assert_eq!(data, [3, 3, 3, 3, 3, 3]);
    }
}
