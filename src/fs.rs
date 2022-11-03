use fake::faker::lorem::en::Words;
use fake::{Dummy, Fake, Faker};
use rand::Rng;
use tempfile::{NamedTempFile, TempPath};

pub enum TempFileKind {
    Text,
}

pub struct TempFileFaker<L = Faker> {
    kind: TempFileKind,
    len: L,
    include_content: bool,
}

impl TempFileFaker<Faker> {
    pub fn new() -> TempFileFaker<Faker> {
        TempFileFaker {
            kind: TempFileKind::Text,
            len: Faker,
            include_content: true,
        }
    }
}

impl<T> TempFileFaker<T> {
    pub fn with_len(len: T) -> Self {
        TempFileFaker {
            kind: TempFileKind::Text,
            len,
            include_content: true,
        }
    }

    pub fn kind(mut self, kind: TempFileKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn include_content(mut self, with_content: bool) -> Self {
        self.include_content = with_content;
        self
    }

    pub fn len<U>(self, len: U) -> TempFileFaker<U> {
        TempFileFaker::<U> {
            kind: self.kind,
            len,
            include_content: self.include_content,
        }
    }
}

pub struct TempFile {
    pub path: TempPath,
    pub content: Option<Vec<u8>>,
}

impl<L> Dummy<TempFileFaker<L>> for TempFile
where
    u8: Dummy<L>,
{
    fn dummy_with_rng<R: Rng + ?Sized>(config: &TempFileFaker<L>, mut rng: &mut R) -> Self {
        let len = config.len.fake_with_rng::<u8, R>(rng) as usize;
        let content = fake_content(&config.kind, len, &mut rng);

        let path = NamedTempFile::new().unwrap().into_temp_path();
        std::fs::write(&path, &content).unwrap();

        TempFile {
            path,
            content: if config.include_content {
                Some(content)
            } else {
                None
            },
        }
    }
}

impl<L> Dummy<TempFileFaker<L>> for TempPath
where
    u8: Dummy<L>,
{
    fn dummy_with_rng<R: Rng + ?Sized>(config: &TempFileFaker<L>, rng: &mut R) -> Self {
        config.fake_with_rng::<TempFile, R>(rng).path
    }
}

pub(crate) fn fake_content<R: Rng + ?Sized>(
    kind: &TempFileKind,
    len: usize,
    rng: &mut R,
) -> Vec<u8> {
    match kind {
        TempFileKind::Text => Words(len..len + 1)
            .fake_with_rng::<Vec<String>, R>(rng)
            .join(" ")
            .into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fake_temp_file_new_with_content() {
        let temp_path: std::path::PathBuf;
        {
            let range = 20..40;
            let faker = TempFileFaker::with_len(range.clone())
                .kind(TempFileKind::Text)
                .include_content(true);
            let temp_file = faker.fake::<TempFile>();
            temp_path = temp_file.path.to_path_buf();

            assert!(temp_path.exists());
            assert!(temp_file.content.is_some());

            let returned_content = temp_file.content.unwrap();
            let words = returned_content
                .iter()
                .fold(0, |cnt, c| if c == &32u8 { cnt + 1 } else { cnt });
            assert!(range.contains(&words));

            let content = std::fs::read_to_string(&temp_path).unwrap().into_bytes();
            assert_eq!(returned_content, content);
        }
        assert!(!temp_path.exists());
    }

    #[test]
    fn test_fake_temp_file_new_without_content() {
        let temp_path: std::path::PathBuf;
        {
            let range = 20..40;
            let faker = TempFileFaker::with_len(range.clone())
                .kind(TempFileKind::Text)
                .include_content(false);
            let temp_file = faker.fake::<TempFile>();
            temp_path = temp_file.path.to_path_buf();

            assert!(temp_path.exists());
            assert!(temp_file.content.is_none());

            let content = std::fs::read_to_string(&temp_path).unwrap().into_bytes();
            let words = content
                .iter()
                .fold(0, |cnt, c| if c == &32u8 { cnt + 1 } else { cnt });
            assert!(range.contains(&words));
        }
        assert!(!temp_path.exists());
    }

    #[test]
    fn test_fake_temp_file_with() {
        let temp_path: std::path::PathBuf;
        {
            let faker = TempFileFaker::new()
                .kind(TempFileKind::Text)
                .include_content(true);
            let temp_file = faker.fake::<TempFile>();
            temp_path = temp_file.path.to_path_buf();

            assert!(temp_path.exists());
            assert!(temp_file.content.is_some());

            let returned_content = temp_file.content.unwrap();
            let content = std::fs::read_to_string(&temp_path).unwrap().into_bytes();
            assert_eq!(returned_content, content);
        }
        assert!(!temp_path.exists());
    }

    #[test]
    fn test_fake_temp_path() {
        let temp_path: std::path::PathBuf;
        {
            let faker = TempFileFaker::new()
                .kind(TempFileKind::Text)
                .include_content(true);
            let temp_path_inst = faker.fake::<TempPath>();
            temp_path = temp_path_inst.to_path_buf();

            assert!(temp_path.exists());
        }
        assert!(!temp_path.exists());
    }
}
