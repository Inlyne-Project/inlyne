use std::{
    cmp::Ordering,
    time::{Duration, SystemTime},
};

use crate::image::{
    cache::{global::RemoteMeta, RemoteKey, StoredImage},
    ImageData,
};

use http_cache_semantics::CachePolicy;
use serde::{Deserialize, Serialize};

macro_rules! default_value_impl {
    ($ty:ty) => {
        impl ::redb::Value for $ty {
            types!();
            fn_fixed_width!(None);
            fn_type_name!(stringify!($ty));
            bincode_byte_fns!();
        }
    };
}

macro_rules! bincode_byte_fns {
    () => {
        fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
        where
            Self: 'a + 'b,
        {
            bincode::serialize(value).unwrap()
        }

        fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
        where
            Self: 'a,
        {
            bincode::deserialize(data).unwrap()
        }
    };
}

// Unlikely to hit us, but we do follow this:
//
// > It is recommended that `name` be prefixed with the crate name to minimize the chance of it
// > coliding with another user defined type
macro_rules! fn_type_name {
    ($ty_name:expr) => {
        fn type_name() -> ::redb::TypeName {
            ::redb::TypeName::new(concat!(env!("CARGO_PKG_NAME"), "-", $ty_name))
        }
    };
}

macro_rules! types {
    () => {
        types!(self_type: Self, as_bytes: Vec<u8>);
    };
    (self_type: $self_ty:ty, as_bytes: $bytes_ty:ty) => {
        type SelfType<'a> = $self_ty where Self: 'a;
        type AsBytes<'a> = $bytes_ty where Self: 'a;
    };
}

macro_rules! fn_fixed_width {
    ($ret:expr) => {
        fn fixed_width() -> Option<usize> {
            $ret
        }
    };
}

#[derive(Debug)]
pub struct SystemTimeWrapper(SystemTime);

impl From<SystemTime> for SystemTimeWrapper {
    fn from(inner: SystemTime) -> Self {
        Self(inner)
    }
}

const LEN: usize = 8;
type SystemTimeBytes = [u8; LEN];

impl redb::Value for SystemTimeWrapper {
    types!(self_type: Self, as_bytes: SystemTimeBytes);
    fn_fixed_width!(Some(LEN));
    fn_type_name!("SystemTimeWrapper");

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        let offset = value
            .0
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Stop time traveling");
        offset.as_secs().to_be_bytes()
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        let bytes = SystemTimeBytes::try_from(data).expect("Should match `as_bytes` len");
        let num_secs = u64::from_be_bytes(bytes);
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(num_secs);
        Self(time)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CachePolicyWrapper(CachePolicy);

impl From<CachePolicy> for CachePolicyWrapper {
    fn from(inner: CachePolicy) -> Self {
        Self(inner)
    }
}

default_value_impl!(CachePolicyWrapper);
default_value_impl!(RemoteKey);

impl redb::Key for RemoteKey {
    fn compare(data1: &[u8], data2: &[u8]) -> Ordering {
        // Seems a bit odd to unwrap here, but it's what `redb` does for `&str`s internally...
        let data1 = std::str::from_utf8(data1).unwrap();
        let data2 = std::str::from_utf8(data2).unwrap();
        data1.cmp(&data2)
    }
}

type FlatRemoteMeta = (SystemTimeWrapper, CachePolicyWrapper);

impl redb::Value for RemoteMeta {
    types!();
    fn_fixed_width!(None);
    fn_type_name!("RemoteMeta");

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        let flat: FlatRemoteMeta = (value.last_used.into(), value.policy.clone().into());
        FlatRemoteMeta::as_bytes(&flat)
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        let (last_used, policy) = <FlatRemoteMeta as redb::Value>::from_bytes(data);
        let last_used = last_used.0;
        let policy = policy.0;
        Self { last_used, policy }
    }
}

// We do a little proxying through another type here because
// 1) we can work with a reference instead of copying
// -- and --
// 2) `Arc<[u8]>` isn't supported for (de)serialization, but slices are
#[derive(Deserialize, Serialize)]
pub struct ImageDataRef<'a> {
    #[serde(borrow)]
    lz4_blob: &'a [u8],
    scale: bool,
    dimensions: (u32, u32),
}

impl<'a> From<ImageDataRef<'a>> for ImageData {
    fn from(image_ref: ImageDataRef) -> Self {
        let ImageDataRef {
            lz4_blob,
            scale,
            dimensions,
        } = image_ref;
        Self {
            lz4_blob: lz4_blob.into(),
            scale,
            dimensions,
        }
    }
}

impl<'a> From<&'a ImageData> for ImageDataRef<'a> {
    fn from(data: &'a ImageData) -> Self {
        let ImageData {
            lz4_blob,
            scale,
            dimensions,
        } = data;
        Self {
            lz4_blob,
            scale: *scale,
            dimensions: *dimensions,
        }
    }
}

impl redb::Value for ImageData {
    types!();
    fn_fixed_width!(None);
    fn_type_name!("ImageData");

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        let image_ref: ImageDataRef = value.into();
        bincode::serialize(&image_ref).unwrap()
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        bincode::deserialize::<ImageDataRef>(data).unwrap().into()
    }
}

// Same deal as `ImageDataRef`. More proxying
#[derive(Deserialize, Serialize)]
pub enum StoredImageRef<'a> {
    #[serde(borrow)]
    PreDecoded(ImageDataRef<'a>),
    #[serde(borrow)]
    CompressedSvg(&'a [u8]),
}

impl<'a> From<StoredImageRef<'a>> for StoredImage {
    fn from(stored_ref: StoredImageRef<'a>) -> Self {
        match stored_ref {
            StoredImageRef::PreDecoded(image_ref) => Self::PreDecoded(image_ref.into()),
            StoredImageRef::CompressedSvg(bytes) => Self::CompressedSvg(bytes.into()),
        }
    }
}

impl<'a> From<&'a StoredImage> for StoredImageRef<'a> {
    fn from(stored: &'a StoredImage) -> Self {
        match &stored {
            StoredImage::PreDecoded(image) => Self::PreDecoded(image.into()),
            StoredImage::CompressedSvg(bytes) => Self::CompressedSvg(bytes),
        }
    }
}

impl redb::Value for StoredImage {
    types!();
    fn_fixed_width!(None);
    fn_type_name!("StoredImage");

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        let stored_ref: StoredImageRef = value.into();
        bincode::serialize(&stored_ref).unwrap()
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        bincode::deserialize::<StoredImageRef>(data).unwrap().into()
    }
}
