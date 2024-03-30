use crate::image::{
    cache::{headers::ETag, Key, Validation, ValidationKind},
    ImageData,
};

use serde::{Deserialize, Serialize};

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

impl<'key> redb::Value for Key<'key> {
    types!(self_type: Key<'a>, as_bytes: &'a str);
    fn_fixed_width!(<&str>::fixed_width());
    fn_type_name!("Key");

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        data.try_into().unwrap()
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a + 'b,
    {
        &value.0
    }
}

impl redb::Value for Validation {
    types!(self_type: Validation, as_bytes: Vec<u8>);
    fn_fixed_width!(None);
    fn_type_name!("Validation");
    bincode_byte_fns!();
}

impl redb::Value for ValidationKind {
    types!(self_type: ValidationKind, as_bytes: Vec<u8>);
    fn_fixed_width!(None);
    fn_type_name!("ValidationKind");
    bincode_byte_fns!();
}

impl redb::Value for ETag {
    types!(self_type: ETag, as_bytes: Vec<u8>);
    fn_fixed_width!(None);
    fn_type_name!("ETag");
    bincode_byte_fns!();
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
    types!(self_type: ImageData, as_bytes: Vec<u8>);
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
