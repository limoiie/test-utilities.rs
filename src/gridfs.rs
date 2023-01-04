use std::cell::RefCell;

use fake::faker::filesystem::en::FileName;
use fake::{Dummy, Fake, Faker};
use mongodb::bson::oid::ObjectId;
use mongodb_gridfs::GridFSBucket;
use rand::Rng;

use crate::fs::{fake_content, TempFileKind};

pub struct TempFileFaker<L = Faker> {
    kind: TempFileKind,
    name: String,
    len: L,
    include_content: bool,
    bucket: RefCell<GridFSBucket>,
}

impl TempFileFaker<Faker> {
    pub fn with_bucket(bucket: GridFSBucket) -> Self {
        TempFileFaker {
            kind: TempFileKind::Text,
            name: FileName().fake(),
            len: Faker,
            include_content: false,
            bucket: RefCell::new(bucket),
        }
    }
}

impl<L> TempFileFaker<L> {
    pub fn kind(self, kind: TempFileKind) -> Self {
        Self { kind, ..self }
    }

    pub fn name(self, name: String) -> Self {
        Self { name, ..self }
    }

    pub fn len<U>(self, len: U) -> TempFileFaker<U> {
        TempFileFaker {
            kind: self.kind,
            name: self.name,
            len,
            include_content: self.include_content,
            bucket: self.bucket,
        }
    }

    pub fn include_content(self, include_content: bool) -> Self {
        Self {
            include_content,
            ..self
        }
    }
}

pub struct TempFile {
    pub id: ObjectId,
    pub filename: Option<String>,
    pub content: Option<Vec<u8>>,
}

impl<L> Dummy<TempFileFaker<L>> for TempFile
where
    usize: Dummy<L>,
{
    fn dummy_with_rng<R: Rng + ?Sized>(config: &TempFileFaker<L>, mut rng: &mut R) -> Self {
        let len = config.len.fake_with_rng::<usize, R>(rng);
        let content = fake_content(&config.kind, len, &mut rng);

        let mut bucket = config.bucket.borrow_mut();
        let oid_fut = bucket.upload_from_stream(&config.name, content.as_slice(), None);

        TempFile {
            id: futures::executor::block_on(oid_fut).unwrap(),
            filename: Some(config.name.clone()),
            content: if config.include_content {
                Some(content)
            } else {
                None
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use mongodb::Client;

    use crate::docker::Builder as ContainerBuilder;

    use super::*;

    #[tokio::test]
    async fn test_fake_temp_file() {
        let handler = ContainerBuilder::new("mongo")
            .bind_port_as_default(Some("0"), "27017")
            .build_disposable()
            .await;
        let db = Client::with_uri_str(handler.url())
            .await
            .unwrap()
            .database("testdb");
        let bucket = GridFSBucket::new(db, None);
        let range = 20..40;
        let faker = TempFileFaker::with_bucket(bucket.clone())
            .kind(TempFileKind::Text)
            .len(range.clone())
            .include_content(true);
        let temp_file = faker.fake::<TempFile>();

        let (mut cursor, cloud_filename) = bucket
            .open_download_stream_with_filename(temp_file.id)
            .await
            .unwrap();
        let cloud_content: Vec<u8> = cursor.next().await.unwrap();

        assert_eq!(cloud_filename, temp_file.filename.unwrap());
        assert_eq!(cloud_content, temp_file.content.unwrap());
    }
}
