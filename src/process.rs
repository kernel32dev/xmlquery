use either::Either;
use futures::{future::join_all, FutureExt};
use std::future::Future;
use std::{fs::Metadata, io::Read};
use zip::read::ZipFile;

pub struct ProcessedFile<'a> {
    path: &'a str,
    meta: Either<Metadata, ZipFile<'a>>,
}

impl<'a> ProcessedFile<'a> {
    pub fn path<'s, 'o: 'a + 's>(&'s self) -> String {
        match &self.meta {
            Either::Left(_) => self.path.to_owned(),
            Either::Right(zipfile) => format!("{}/{}", self.path, zipfile.name()),
        }
    }
    pub fn read_to_string(self) -> Result<String, ()> {
        match self.meta {
            Either::Left(_) => std::fs::read_to_string(&self.path).map_err(catch_io(&self.path)),
            Either::Right(mut zipfile) => {
                let mut buf = String::with_capacity(zipfile.size() as usize);
                zipfile
                    .read_to_string(&mut buf)
                    .map_err(catch_io(zipfile.name()))?;
                Ok(buf)
            }
        }
    }
}

pub async fn process_paths<F: Future<Output = ()>>(
    paths: impl IntoIterator<Item = impl AsRef<str>>,
    callback: impl for<'a> Fn(ProcessedFile<'a>) -> F + Clone + Send + 'static,
) {
    join_all(paths.into_iter().map(|path| {
        let callback = callback.clone();
        async move {
            process_path(path.as_ref().to_owned(), callback).await;
        }
    }))
    .await;
}

pub async fn process_path<F: Future<Output = ()>>(
    path: String,
    callback: impl for<'a> Fn(ProcessedFile<'a>) -> F + Clone + Send + 'static,
) {
    let Ok(metadata) = tokio::fs::metadata(&path).await.map_err(catch_io(&path)) else {
        return;
    };
    if metadata.is_dir() {
        process_folder(path, callback.clone()).await;
    } else if metadata.is_file() {
        process_file(path, metadata, callback.clone()).await;
    } else {
        unreachable!()
    }
}

pub async fn process_folder<F: Future<Output = ()>>(
    path: String,
    callback: impl for<'a> Fn(ProcessedFile<'a>) -> F + Clone + Send + 'static,
) {
    let _ = async {
        let mut dir = tokio::fs::read_dir(&path).await?;
        let mut set = Vec::new();
        while let Some(next) = dir.next_entry().await? {
            let metadata = next.metadata().await?;
            let name = next.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let path = format!("{}/{}", path, name);
            if metadata.is_dir() {
                set.push(process_folder(path, callback.clone()).left_future());
            } else if metadata.is_file() {
                set.push(process_file(path, metadata, callback.clone()).right_future());
            } else {
                unreachable!()
            }
        }
        join_all(set).await;
        Ok(())
    }
    .await
    .map_err(catch_io(&path));
}

pub async fn process_file<F: Future<Output = ()>>(
    path: String,
    metadata: Metadata,
    callback: impl for<'a> Fn(ProcessedFile<'a>) -> F + Clone + Send + 'static,
) {
    if path.ends_with(".zip") {
        tokio::task::spawn_blocking(move || {
            let _ = (|| {
                let file = std::fs::File::open(&path)?;
                let mut zip = zip::ZipArchive::new(file)?;
                for i in 0..zip.len() {
                    let zipfile = zip.by_index(i)?;
                    futures::executor::block_on(callback(ProcessedFile {
                        path: &path,
                        meta: Either::Right(zipfile),
                    }));
                }
                Ok(())
            })()
            .map_err(catch_io(&path));
        })
        .await
        .unwrap();
    } else {
        callback(ProcessedFile {
            path: &path,
            meta: Either::Left(metadata),
        })
        .await
    }
}

const fn catch_io<'a>(path: &'a str) -> impl FnOnce(std::io::Error) + 'a {
    move |error| {
        eprintln!("{path}: {error:#?}");
    }
}
