use std::cell::RefCell;

use fake::{Dummy, Fake, Faker};
use fake::faker::filesystem::en::FileName;
use mongodb::bson::oid::ObjectId;
use mongodb_gridfs::GridFSBucket;
use rand::Rng;

use crate::fs::{fake_content, TempFileKind};

pub struct TempFileFaker<L> {
    kind: TempFileKind,
    name: String,
    len: L,
    with_content: bool,
    bucket: RefCell<GridFSBucket>,
}

impl TempFileFaker<Faker> {
    pub fn with(kind: TempFileKind, bucket: GridFSBucket, name: Option<String>, with_content: bool)
                -> Self {
        let name = name.unwrap_or_else(|| FileName().fake());
        TempFileFaker { kind, name, len: Faker, with_content, bucket: RefCell::new(bucket) }
    }
}

impl<L> TempFileFaker<L> {
    pub fn with_len(kind: TempFileKind, bucket: GridFSBucket, name: Option<String>, len: L,
                    with_content: bool) -> Self {
        let name = name.unwrap_or_else(|| FileName().fake());
        TempFileFaker { kind, name, len, with_content, bucket: RefCell::new(bucket) }
    }
}

pub struct TempFile {
    pub id: ObjectId,
    pub filename: Option<String>,
    pub content: Option<Vec<u8>>,
}

impl<L> Dummy<TempFileFaker<L>> for TempFile
    where
        u8: Dummy<L>,
{
    fn dummy_with_rng<R: Rng + ?Sized>(config: &TempFileFaker<L>, mut rng: &mut R) -> Self {
        let len = config.len.fake_with_rng::<u8, R>(rng) as usize;
        let content = fake_content(&config.kind, len, &mut rng);

        let mut bucket = config.bucket.borrow_mut();
        let oid_fut = bucket.upload_from_stream(&config.name, content.as_slice(), None);

        TempFile {
            id: futures::executor::block_on(oid_fut).unwrap(),
            filename: Some(config.name.clone()),
            content: if config.with_content { Some(content) } else { None },
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
            .port_mapping(30017, Some(27017))
            .build_disposable()
            .await;
        let db = Client::with_uri_str(handler.url.as_ref().unwrap())
            .await.unwrap()
            .database("testdb");
        let bucket = GridFSBucket::new(db, None);
        let range = 20..40;
        let faker = TempFileFaker::with_len(
            TempFileKind::Text,
            bucket.clone(),
            None,
            range.clone(),
            true,
        );
        let temp_file = faker.fake::<TempFile>();

        let (mut cursor, cloud_filename) =
            bucket.open_download_stream_with_filename(temp_file.id).await.unwrap();
        let cloud_content: Vec<u8> = cursor.next().await.unwrap();

        assert_eq!(cloud_filename, temp_file.filename.unwrap());
        assert_eq!(cloud_content, temp_file.content.unwrap());
    }
}
