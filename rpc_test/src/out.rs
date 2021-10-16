#![feature(prelude_import)]
#[prelude_import]
use std::prelude::rust_2018::*;
#[macro_use]
extern crate std;
pub enum MyError {
    Error1,
    Call(remoc::rtc::CallError),
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl _serde::Serialize for MyError {
        fn serialize<__S>(
            &self,
            __serializer: __S,
        ) -> _serde::__private::Result<__S::Ok, __S::Error>
        where
            __S: _serde::Serializer,
        {
            match *self {
                MyError::Error1 => _serde::Serializer::serialize_unit_variant(
                    __serializer,
                    "MyError",
                    0u32,
                    "Error1",
                ),
                MyError::Call(ref __field0) => _serde::Serializer::serialize_newtype_variant(
                    __serializer,
                    "MyError",
                    1u32,
                    "Call",
                    __field0,
                ),
            }
        }
    }
};
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<'de> _serde::Deserialize<'de> for MyError {
        fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
        where
            __D: _serde::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            enum __Field {
                __field0,
                __field1,
            }
            struct __FieldVisitor;
            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                type Value = __Field;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "variant identifier")
                }
                fn visit_u64<__E>(self, __value: u64) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        0u64 => _serde::__private::Ok(__Field::__field0),
                        1u64 => _serde::__private::Ok(__Field::__field1),
                        _ => _serde::__private::Err(_serde::de::Error::invalid_value(
                            _serde::de::Unexpected::Unsigned(__value),
                            &"variant index 0 <= i < 2",
                        )),
                    }
                }
                fn visit_str<__E>(
                    self,
                    __value: &str,
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "Error1" => _serde::__private::Ok(__Field::__field0),
                        "Call" => _serde::__private::Ok(__Field::__field1),
                        _ => _serde::__private::Err(_serde::de::Error::unknown_variant(
                            __value, VARIANTS,
                        )),
                    }
                }
                fn visit_bytes<__E>(
                    self,
                    __value: &[u8],
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"Error1" => _serde::__private::Ok(__Field::__field0),
                        b"Call" => _serde::__private::Ok(__Field::__field1),
                        _ => {
                            let __value = &_serde::__private::from_utf8_lossy(__value);
                            _serde::__private::Err(_serde::de::Error::unknown_variant(
                                __value, VARIANTS,
                            ))
                        }
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(
                    __deserializer: __D,
                ) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            struct __Visitor<'de> {
                marker: _serde::__private::PhantomData<MyError>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                type Value = MyError;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "enum MyError")
                }
                fn visit_enum<__A>(
                    self,
                    __data: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::EnumAccess<'de>,
                {
                    match match _serde::de::EnumAccess::variant(__data) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    } {
                        (__Field::__field0, __variant) => {
                            match _serde::de::VariantAccess::unit_variant(__variant) {
                                _serde::__private::Ok(__val) => __val,
                                _serde::__private::Err(__err) => {
                                    return _serde::__private::Err(__err);
                                }
                            };
                            _serde::__private::Ok(MyError::Error1)
                        }
                        (__Field::__field1, __variant) => _serde::__private::Result::map(
                            _serde::de::VariantAccess::newtype_variant::<remoc::rtc::CallError>(
                                __variant,
                            ),
                            MyError::Call,
                        ),
                    }
                }
            }
            const VARIANTS: &'static [&'static str] = &["Error1", "Call"];
            _serde::Deserializer::deserialize_enum(
                __deserializer,
                "MyError",
                VARIANTS,
                __Visitor {
                    marker: _serde::__private::PhantomData::<MyError>,
                    lifetime: _serde::__private::PhantomData,
                },
            )
        }
    }
};
impl From<remoc::rtc::CallError> for MyError {
    fn from(err: remoc::rtc::CallError) -> Self {
        Self::Call(err)
    }
}
pub trait MyService<Codec> {
    /// Const fn docs.
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    fn const_fn<'life0, 'async_trait>(
        &'life0 self,
        arg1: String,
        arg2: u16,
        arg3: remoc::rch::mpsc::Sender<String, Codec>,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<u32, MyError>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait;
    /// Mut fn docs.
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    fn mut_fn<'life0, 'async_trait>(
        &'life0 mut self,
        arg1: Vec<String>,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<(), MyError>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait;
}
#[serde(bound(serialize = "Codec: ::remoc::codec::Codec"))]
#[serde(bound(deserialize = "Codec: ::remoc::codec::Codec"))]
enum MyServiceReqValue<Codec> {
    __Phantom(::std::marker::PhantomData<(Codec)>),
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<Codec> _serde::Serialize for MyServiceReqValue<Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn serialize<__S>(
            &self,
            __serializer: __S,
        ) -> _serde::__private::Result<__S::Ok, __S::Error>
        where
            __S: _serde::Serializer,
        {
            match *self {
                MyServiceReqValue::__Phantom(ref __field0) => {
                    _serde::Serializer::serialize_newtype_variant(
                        __serializer,
                        "MyServiceReqValue",
                        0u32,
                        "__Phantom",
                        __field0,
                    )
                }
            }
        }
    }
};
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<'de, Codec> _serde::Deserialize<'de> for MyServiceReqValue<Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
        where
            __D: _serde::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            enum __Field {
                __field0,
            }
            struct __FieldVisitor;
            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                type Value = __Field;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "variant identifier")
                }
                fn visit_u64<__E>(self, __value: u64) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        0u64 => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Err(_serde::de::Error::invalid_value(
                            _serde::de::Unexpected::Unsigned(__value),
                            &"variant index 0 <= i < 1",
                        )),
                    }
                }
                fn visit_str<__E>(
                    self,
                    __value: &str,
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "__Phantom" => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Err(_serde::de::Error::unknown_variant(
                            __value, VARIANTS,
                        )),
                    }
                }
                fn visit_bytes<__E>(
                    self,
                    __value: &[u8],
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"__Phantom" => _serde::__private::Ok(__Field::__field0),
                        _ => {
                            let __value = &_serde::__private::from_utf8_lossy(__value);
                            _serde::__private::Err(_serde::de::Error::unknown_variant(
                                __value, VARIANTS,
                            ))
                        }
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(
                    __deserializer: __D,
                ) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            struct __Visitor<'de, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                marker: _serde::__private::PhantomData<MyServiceReqValue<Codec>>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de, Codec> _serde::de::Visitor<'de> for __Visitor<'de, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                type Value = MyServiceReqValue<Codec>;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "enum MyServiceReqValue")
                }
                fn visit_enum<__A>(
                    self,
                    __data: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::EnumAccess<'de>,
                {
                    match match _serde::de::EnumAccess::variant(__data) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    } {
                        (__Field::__field0, __variant) => _serde::__private::Result::map(
                            _serde::de::VariantAccess::newtype_variant::<
                                ::std::marker::PhantomData<(Codec)>,
                            >(__variant),
                            MyServiceReqValue::__Phantom,
                        ),
                    }
                }
            }
            const VARIANTS: &'static [&'static str] = &["__Phantom"];
            _serde::Deserializer::deserialize_enum(
                __deserializer,
                "MyServiceReqValue",
                VARIANTS,
                __Visitor {
                    marker: _serde::__private::PhantomData::<MyServiceReqValue<Codec>>,
                    lifetime: _serde::__private::PhantomData,
                },
            )
        }
    }
};
impl<Codec> MyServiceReqValue<Codec>
where
    Codec: ::remoc::codec::Codec,
{
    async fn dispatch<Target>(self, target: Target)
    where
        Target: MyService<Codec>,
    {
        match self {
            Self::__Phantom(_) => (),
        }
    }
}
#[serde(bound(serialize = "Codec: ::remoc::codec::Codec"))]
#[serde(bound(deserialize = "Codec: ::remoc::codec::Codec"))]
enum MyServiceReqRef<Codec> {
    ConstFn {
        __reply_tx: ::remoc::rch::oneshot::Sender<Result<u32, MyError>, Codec>,
        arg1: String,
        arg2: u16,
        arg3: remoc::rch::mpsc::Sender<String, Codec>,
    },
    __Phantom(::std::marker::PhantomData<(Codec)>),
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<Codec> _serde::Serialize for MyServiceReqRef<Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn serialize<__S>(
            &self,
            __serializer: __S,
        ) -> _serde::__private::Result<__S::Ok, __S::Error>
        where
            __S: _serde::Serializer,
        {
            match *self {
                MyServiceReqRef::ConstFn {
                    ref __reply_tx,
                    ref arg1,
                    ref arg2,
                    ref arg3,
                } => {
                    let mut __serde_state = match _serde::Serializer::serialize_struct_variant(
                        __serializer,
                        "MyServiceReqRef",
                        0u32,
                        "ConstFn",
                        0 + 1 + 1 + 1 + 1,
                    ) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    };
                    match _serde::ser::SerializeStructVariant::serialize_field(
                        &mut __serde_state,
                        "__reply_tx",
                        __reply_tx,
                    ) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    };
                    match _serde::ser::SerializeStructVariant::serialize_field(
                        &mut __serde_state,
                        "arg1",
                        arg1,
                    ) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    };
                    match _serde::ser::SerializeStructVariant::serialize_field(
                        &mut __serde_state,
                        "arg2",
                        arg2,
                    ) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    };
                    match _serde::ser::SerializeStructVariant::serialize_field(
                        &mut __serde_state,
                        "arg3",
                        arg3,
                    ) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    };
                    _serde::ser::SerializeStructVariant::end(__serde_state)
                }
                MyServiceReqRef::__Phantom(ref __field0) => {
                    _serde::Serializer::serialize_newtype_variant(
                        __serializer,
                        "MyServiceReqRef",
                        1u32,
                        "__Phantom",
                        __field0,
                    )
                }
            }
        }
    }
};
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<'de, Codec> _serde::Deserialize<'de> for MyServiceReqRef<Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
        where
            __D: _serde::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            enum __Field {
                __field0,
                __field1,
            }
            struct __FieldVisitor;
            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                type Value = __Field;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "variant identifier")
                }
                fn visit_u64<__E>(self, __value: u64) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        0u64 => _serde::__private::Ok(__Field::__field0),
                        1u64 => _serde::__private::Ok(__Field::__field1),
                        _ => _serde::__private::Err(_serde::de::Error::invalid_value(
                            _serde::de::Unexpected::Unsigned(__value),
                            &"variant index 0 <= i < 2",
                        )),
                    }
                }
                fn visit_str<__E>(
                    self,
                    __value: &str,
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "ConstFn" => _serde::__private::Ok(__Field::__field0),
                        "__Phantom" => _serde::__private::Ok(__Field::__field1),
                        _ => _serde::__private::Err(_serde::de::Error::unknown_variant(
                            __value, VARIANTS,
                        )),
                    }
                }
                fn visit_bytes<__E>(
                    self,
                    __value: &[u8],
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"ConstFn" => _serde::__private::Ok(__Field::__field0),
                        b"__Phantom" => _serde::__private::Ok(__Field::__field1),
                        _ => {
                            let __value = &_serde::__private::from_utf8_lossy(__value);
                            _serde::__private::Err(_serde::de::Error::unknown_variant(
                                __value, VARIANTS,
                            ))
                        }
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(
                    __deserializer: __D,
                ) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            struct __Visitor<'de, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                marker: _serde::__private::PhantomData<MyServiceReqRef<Codec>>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de, Codec> _serde::de::Visitor<'de> for __Visitor<'de, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                type Value = MyServiceReqRef<Codec>;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "enum MyServiceReqRef")
                }
                fn visit_enum<__A>(
                    self,
                    __data: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::EnumAccess<'de>,
                {
                    match match _serde::de::EnumAccess::variant(__data) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    } {
                        (__Field::__field0, __variant) => {
                            #[allow(non_camel_case_types)]
                            enum __Field {
                                __field0,
                                __field1,
                                __field2,
                                __field3,
                                __ignore,
                            }
                            struct __FieldVisitor;
                            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                                type Value = __Field;
                                fn expecting(
                                    &self,
                                    __formatter: &mut _serde::__private::Formatter,
                                ) -> _serde::__private::fmt::Result
                                {
                                    _serde::__private::Formatter::write_str(
                                        __formatter,
                                        "field identifier",
                                    )
                                }
                                fn visit_u64<__E>(
                                    self,
                                    __value: u64,
                                ) -> _serde::__private::Result<Self::Value, __E>
                                where
                                    __E: _serde::de::Error,
                                {
                                    match __value {
                                        0u64 => _serde::__private::Ok(__Field::__field0),
                                        1u64 => _serde::__private::Ok(__Field::__field1),
                                        2u64 => _serde::__private::Ok(__Field::__field2),
                                        3u64 => _serde::__private::Ok(__Field::__field3),
                                        _ => _serde::__private::Ok(__Field::__ignore),
                                    }
                                }
                                fn visit_str<__E>(
                                    self,
                                    __value: &str,
                                ) -> _serde::__private::Result<Self::Value, __E>
                                where
                                    __E: _serde::de::Error,
                                {
                                    match __value {
                                        "__reply_tx" => _serde::__private::Ok(__Field::__field0),
                                        "arg1" => _serde::__private::Ok(__Field::__field1),
                                        "arg2" => _serde::__private::Ok(__Field::__field2),
                                        "arg3" => _serde::__private::Ok(__Field::__field3),
                                        _ => _serde::__private::Ok(__Field::__ignore),
                                    }
                                }
                                fn visit_bytes<__E>(
                                    self,
                                    __value: &[u8],
                                ) -> _serde::__private::Result<Self::Value, __E>
                                where
                                    __E: _serde::de::Error,
                                {
                                    match __value {
                                        b"__reply_tx" => _serde::__private::Ok(__Field::__field0),
                                        b"arg1" => _serde::__private::Ok(__Field::__field1),
                                        b"arg2" => _serde::__private::Ok(__Field::__field2),
                                        b"arg3" => _serde::__private::Ok(__Field::__field3),
                                        _ => _serde::__private::Ok(__Field::__ignore),
                                    }
                                }
                            }
                            impl<'de> _serde::Deserialize<'de> for __Field {
                                #[inline]
                                fn deserialize<__D>(
                                    __deserializer: __D,
                                ) -> _serde::__private::Result<Self, __D::Error>
                                where
                                    __D: _serde::Deserializer<'de>,
                                {
                                    _serde::Deserializer::deserialize_identifier(
                                        __deserializer,
                                        __FieldVisitor,
                                    )
                                }
                            }
                            struct __Visitor<'de, Codec>
                            where
                                Codec: ::remoc::codec::Codec,
                            {
                                marker: _serde::__private::PhantomData<MyServiceReqRef<Codec>>,
                                lifetime: _serde::__private::PhantomData<&'de ()>,
                            }
                            impl<'de, Codec> _serde::de::Visitor<'de> for __Visitor<'de, Codec>
                            where
                                Codec: ::remoc::codec::Codec,
                            {
                                type Value = MyServiceReqRef<Codec>;
                                fn expecting(
                                    &self,
                                    __formatter: &mut _serde::__private::Formatter,
                                ) -> _serde::__private::fmt::Result
                                {
                                    _serde::__private::Formatter::write_str(
                                        __formatter,
                                        "struct variant MyServiceReqRef::ConstFn",
                                    )
                                }
                                #[inline]
                                fn visit_seq<__A>(
                                    self,
                                    mut __seq: __A,
                                ) -> _serde::__private::Result<Self::Value, __A::Error>
                                where
                                    __A: _serde::de::SeqAccess<'de>,
                                {
                                    let __field0 = match match _serde::de::SeqAccess::next_element::<
                                        ::remoc::rch::oneshot::Sender<Result<u32, MyError>, Codec>,
                                    >(
                                        &mut __seq
                                    ) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    } {
                                        _serde::__private::Some(__value) => __value,
                                        _serde::__private::None => {
                                            return _serde :: __private :: Err (_serde :: de :: Error :: invalid_length (0usize , & "struct variant MyServiceReqRef::ConstFn with 4 elements")) ;
                                        }
                                    };
                                    let __field1 = match match _serde::de::SeqAccess::next_element::<
                                        String,
                                    >(
                                        &mut __seq
                                    ) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    } {
                                        _serde::__private::Some(__value) => __value,
                                        _serde::__private::None => {
                                            return _serde :: __private :: Err (_serde :: de :: Error :: invalid_length (1usize , & "struct variant MyServiceReqRef::ConstFn with 4 elements")) ;
                                        }
                                    };
                                    let __field2 = match match _serde::de::SeqAccess::next_element::<
                                        u16,
                                    >(
                                        &mut __seq
                                    ) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    } {
                                        _serde::__private::Some(__value) => __value,
                                        _serde::__private::None => {
                                            return _serde :: __private :: Err (_serde :: de :: Error :: invalid_length (2usize , & "struct variant MyServiceReqRef::ConstFn with 4 elements")) ;
                                        }
                                    };
                                    let __field3 = match match _serde::de::SeqAccess::next_element::<
                                        remoc::rch::mpsc::Sender<String, Codec>,
                                    >(
                                        &mut __seq
                                    ) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    } {
                                        _serde::__private::Some(__value) => __value,
                                        _serde::__private::None => {
                                            return _serde :: __private :: Err (_serde :: de :: Error :: invalid_length (3usize , & "struct variant MyServiceReqRef::ConstFn with 4 elements")) ;
                                        }
                                    };
                                    _serde::__private::Ok(MyServiceReqRef::ConstFn {
                                        __reply_tx: __field0,
                                        arg1: __field1,
                                        arg2: __field2,
                                        arg3: __field3,
                                    })
                                }
                                #[inline]
                                fn visit_map<__A>(
                                    self,
                                    mut __map: __A,
                                ) -> _serde::__private::Result<Self::Value, __A::Error>
                                where
                                    __A: _serde::de::MapAccess<'de>,
                                {
                                    let mut __field0: _serde::__private::Option<
                                        ::remoc::rch::oneshot::Sender<Result<u32, MyError>, Codec>,
                                    > = _serde::__private::None;
                                    let mut __field1: _serde::__private::Option<String> =
                                        _serde::__private::None;
                                    let mut __field2: _serde::__private::Option<u16> =
                                        _serde::__private::None;
                                    let mut __field3: _serde::__private::Option<
                                        remoc::rch::mpsc::Sender<String, Codec>,
                                    > = _serde::__private::None;
                                    while let _serde::__private::Some(__key) =
                                        match _serde::de::MapAccess::next_key::<__Field>(&mut __map)
                                        {
                                            _serde::__private::Ok(__val) => __val,
                                            _serde::__private::Err(__err) => {
                                                return _serde::__private::Err(__err);
                                            }
                                        }
                                    {
                                        match __key {
                                            __Field::__field0 => {
                                                if _serde::__private::Option::is_some(&__field0) {
                                                    return _serde :: __private :: Err (< __A :: Error as _serde :: de :: Error > :: duplicate_field ("__reply_tx")) ;
                                                }
                                                __field0 = _serde::__private::Some(
                                                    match _serde::de::MapAccess::next_value::<
                                                        ::remoc::rch::oneshot::Sender<
                                                            Result<u32, MyError>,
                                                            Codec,
                                                        >,
                                                    >(
                                                        &mut __map
                                                    ) {
                                                        _serde::__private::Ok(__val) => __val,
                                                        _serde::__private::Err(__err) => {
                                                            return _serde::__private::Err(__err);
                                                        }
                                                    },
                                                );
                                            }
                                            __Field::__field1 => {
                                                if _serde::__private::Option::is_some(&__field1) {
                                                    return _serde :: __private :: Err (< __A :: Error as _serde :: de :: Error > :: duplicate_field ("arg1")) ;
                                                }
                                                __field1 = _serde::__private::Some(
                                                    match _serde::de::MapAccess::next_value::<String>(
                                                        &mut __map,
                                                    ) {
                                                        _serde::__private::Ok(__val) => __val,
                                                        _serde::__private::Err(__err) => {
                                                            return _serde::__private::Err(__err);
                                                        }
                                                    },
                                                );
                                            }
                                            __Field::__field2 => {
                                                if _serde::__private::Option::is_some(&__field2) {
                                                    return _serde :: __private :: Err (< __A :: Error as _serde :: de :: Error > :: duplicate_field ("arg2")) ;
                                                }
                                                __field2 = _serde::__private::Some(
                                                    match _serde::de::MapAccess::next_value::<u16>(
                                                        &mut __map,
                                                    ) {
                                                        _serde::__private::Ok(__val) => __val,
                                                        _serde::__private::Err(__err) => {
                                                            return _serde::__private::Err(__err);
                                                        }
                                                    },
                                                );
                                            }
                                            __Field::__field3 => {
                                                if _serde::__private::Option::is_some(&__field3) {
                                                    return _serde :: __private :: Err (< __A :: Error as _serde :: de :: Error > :: duplicate_field ("arg3")) ;
                                                }
                                                __field3 = _serde::__private::Some(
                                                    match _serde::de::MapAccess::next_value::<
                                                        remoc::rch::mpsc::Sender<String, Codec>,
                                                    >(
                                                        &mut __map
                                                    ) {
                                                        _serde::__private::Ok(__val) => __val,
                                                        _serde::__private::Err(__err) => {
                                                            return _serde::__private::Err(__err);
                                                        }
                                                    },
                                                );
                                            }
                                            _ => {
                                                let _ = match _serde::de::MapAccess::next_value::<
                                                    _serde::de::IgnoredAny,
                                                >(
                                                    &mut __map
                                                ) {
                                                    _serde::__private::Ok(__val) => __val,
                                                    _serde::__private::Err(__err) => {
                                                        return _serde::__private::Err(__err);
                                                    }
                                                };
                                            }
                                        }
                                    }
                                    let __field0 = match __field0 {
                                        _serde::__private::Some(__field0) => __field0,
                                        _serde::__private::None => {
                                            match _serde::__private::de::missing_field("__reply_tx")
                                            {
                                                _serde::__private::Ok(__val) => __val,
                                                _serde::__private::Err(__err) => {
                                                    return _serde::__private::Err(__err);
                                                }
                                            }
                                        }
                                    };
                                    let __field1 = match __field1 {
                                        _serde::__private::Some(__field1) => __field1,
                                        _serde::__private::None => {
                                            match _serde::__private::de::missing_field("arg1") {
                                                _serde::__private::Ok(__val) => __val,
                                                _serde::__private::Err(__err) => {
                                                    return _serde::__private::Err(__err);
                                                }
                                            }
                                        }
                                    };
                                    let __field2 = match __field2 {
                                        _serde::__private::Some(__field2) => __field2,
                                        _serde::__private::None => {
                                            match _serde::__private::de::missing_field("arg2") {
                                                _serde::__private::Ok(__val) => __val,
                                                _serde::__private::Err(__err) => {
                                                    return _serde::__private::Err(__err);
                                                }
                                            }
                                        }
                                    };
                                    let __field3 = match __field3 {
                                        _serde::__private::Some(__field3) => __field3,
                                        _serde::__private::None => {
                                            match _serde::__private::de::missing_field("arg3") {
                                                _serde::__private::Ok(__val) => __val,
                                                _serde::__private::Err(__err) => {
                                                    return _serde::__private::Err(__err);
                                                }
                                            }
                                        }
                                    };
                                    _serde::__private::Ok(MyServiceReqRef::ConstFn {
                                        __reply_tx: __field0,
                                        arg1: __field1,
                                        arg2: __field2,
                                        arg3: __field3,
                                    })
                                }
                            }
                            const FIELDS: &'static [&'static str] =
                                &["__reply_tx", "arg1", "arg2", "arg3"];
                            _serde::de::VariantAccess::struct_variant(
                                __variant,
                                FIELDS,
                                __Visitor {
                                    marker: _serde::__private::PhantomData::<MyServiceReqRef<Codec>>,
                                    lifetime: _serde::__private::PhantomData,
                                },
                            )
                        }
                        (__Field::__field1, __variant) => _serde::__private::Result::map(
                            _serde::de::VariantAccess::newtype_variant::<
                                ::std::marker::PhantomData<(Codec)>,
                            >(__variant),
                            MyServiceReqRef::__Phantom,
                        ),
                    }
                }
            }
            const VARIANTS: &'static [&'static str] = &["ConstFn", "__Phantom"];
            _serde::Deserializer::deserialize_enum(
                __deserializer,
                "MyServiceReqRef",
                VARIANTS,
                __Visitor {
                    marker: _serde::__private::PhantomData::<MyServiceReqRef<Codec>>,
                    lifetime: _serde::__private::PhantomData,
                },
            )
        }
    }
};
impl<Codec> MyServiceReqRef<Codec>
where
    Codec: ::remoc::codec::Codec,
{
    async fn dispatch<Target>(self, target: &Target)
    where
        Target: MyService<Codec>,
    {
        match self {
            Self::ConstFn {
                arg1,
                arg2,
                arg3,
                __reply_tx,
            } => {
                mod util {
                    pub(super) enum Out<_0, _1> {
                        _0(_0),
                        _1(_1),
                        Disabled,
                    }
                    pub(super) type Mask = u8;
                }
                use ::tokio::macros::support::Future;
                use ::tokio::macros::support::Pin;
                use ::tokio::macros::support::Poll::{Ready, Pending};
                const BRANCHES: u32 = 2;
                let mut disabled: util::Mask = Default::default();
                if !true {
                    let mask: util::Mask = 1 << 0;
                    disabled |= mask;
                }
                if !true {
                    let mask: util::Mask = 1 << 1;
                    disabled |= mask;
                }
                let mut output = {
                    let mut futures = (__reply_tx.closed(), target.const_fn(arg1, arg2, arg3));
                    ::tokio::macros::support::poll_fn(|cx| {
                        let mut is_pending = false;
                        let start = 0;
                        for i in 0..BRANCHES {
                            let branch;
                            #[allow(clippy::modulo_one)]
                            {
                                branch = (start + i) % BRANCHES;
                            }
                            match branch {
                                #[allow(unreachable_code)]
                                0 => {
                                    let mask = 1 << branch;
                                    if disabled & mask == mask {
                                        continue;
                                    }
                                    let (fut, ..) = &mut futures;
                                    let mut fut = unsafe { Pin::new_unchecked(fut) };
                                    let out = match fut.poll(cx) {
                                        Ready(out) => out,
                                        Pending => {
                                            is_pending = true;
                                            continue;
                                        }
                                    };
                                    disabled |= mask;
                                    #[allow(unused_variables)]
                                    #[allow(unused_mut)]
                                    match &out {
                                        () => {}
                                        _ => continue,
                                    }
                                    return Ready(util::Out::_0(out));
                                }
                                #[allow(unreachable_code)]
                                1 => {
                                    let mask = 1 << branch;
                                    if disabled & mask == mask {
                                        continue;
                                    }
                                    let (_, fut, ..) = &mut futures;
                                    let mut fut = unsafe { Pin::new_unchecked(fut) };
                                    let out = match fut.poll(cx) {
                                        Ready(out) => out,
                                        Pending => {
                                            is_pending = true;
                                            continue;
                                        }
                                    };
                                    disabled |= mask;
                                    #[allow(unused_variables)]
                                    #[allow(unused_mut)]
                                    match &out {
                                        result => {}
                                        _ => continue,
                                    }
                                    return Ready(util::Out::_1(out));
                                }
                                _ => ::core::panicking::panic_fmt(::core::fmt::Arguments::new_v1(
                                    &["internal error: entered unreachable code: "],
                                    &match (
                                        &"reaching this means there probably is an off by one bug",
                                    ) {
                                        (arg0,) => [::core::fmt::ArgumentV1::new(
                                            arg0,
                                            ::core::fmt::Display::fmt,
                                        )],
                                    },
                                )),
                            }
                        }
                        if is_pending {
                            Pending
                        } else {
                            Ready(util::Out::Disabled)
                        }
                    })
                    .await
                };
                match output {
                    util::Out::_0(()) => (),
                    util::Out::_1(result) => {
                        let _ = __reply_tx.send(result);
                    }
                    util::Out::Disabled => ::std::rt::begin_panic(
                        "all branches are disabled and there is no else branch",
                    ),
                    _ => ::core::panicking::panic_fmt(::core::fmt::Arguments::new_v1(
                        &["internal error: entered unreachable code: "],
                        &match (&"failed to match bind",) {
                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                arg0,
                                ::core::fmt::Display::fmt,
                            )],
                        },
                    )),
                }
            }
            Self::__Phantom(_) => (),
        }
    }
}
#[serde(bound(serialize = "Codec: ::remoc::codec::Codec"))]
#[serde(bound(deserialize = "Codec: ::remoc::codec::Codec"))]
enum MyServiceReqRefMut<Codec> {
    MutFn {
        __reply_tx: ::remoc::rch::oneshot::Sender<Result<(), MyError>, Codec>,
        arg1: Vec<String>,
    },
    __Phantom(::std::marker::PhantomData<(Codec)>),
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<Codec> _serde::Serialize for MyServiceReqRefMut<Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn serialize<__S>(
            &self,
            __serializer: __S,
        ) -> _serde::__private::Result<__S::Ok, __S::Error>
        where
            __S: _serde::Serializer,
        {
            match *self {
                MyServiceReqRefMut::MutFn {
                    ref __reply_tx,
                    ref arg1,
                } => {
                    let mut __serde_state = match _serde::Serializer::serialize_struct_variant(
                        __serializer,
                        "MyServiceReqRefMut",
                        0u32,
                        "MutFn",
                        0 + 1 + 1,
                    ) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    };
                    match _serde::ser::SerializeStructVariant::serialize_field(
                        &mut __serde_state,
                        "__reply_tx",
                        __reply_tx,
                    ) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    };
                    match _serde::ser::SerializeStructVariant::serialize_field(
                        &mut __serde_state,
                        "arg1",
                        arg1,
                    ) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    };
                    _serde::ser::SerializeStructVariant::end(__serde_state)
                }
                MyServiceReqRefMut::__Phantom(ref __field0) => {
                    _serde::Serializer::serialize_newtype_variant(
                        __serializer,
                        "MyServiceReqRefMut",
                        1u32,
                        "__Phantom",
                        __field0,
                    )
                }
            }
        }
    }
};
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<'de, Codec> _serde::Deserialize<'de> for MyServiceReqRefMut<Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
        where
            __D: _serde::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            enum __Field {
                __field0,
                __field1,
            }
            struct __FieldVisitor;
            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                type Value = __Field;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "variant identifier")
                }
                fn visit_u64<__E>(self, __value: u64) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        0u64 => _serde::__private::Ok(__Field::__field0),
                        1u64 => _serde::__private::Ok(__Field::__field1),
                        _ => _serde::__private::Err(_serde::de::Error::invalid_value(
                            _serde::de::Unexpected::Unsigned(__value),
                            &"variant index 0 <= i < 2",
                        )),
                    }
                }
                fn visit_str<__E>(
                    self,
                    __value: &str,
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "MutFn" => _serde::__private::Ok(__Field::__field0),
                        "__Phantom" => _serde::__private::Ok(__Field::__field1),
                        _ => _serde::__private::Err(_serde::de::Error::unknown_variant(
                            __value, VARIANTS,
                        )),
                    }
                }
                fn visit_bytes<__E>(
                    self,
                    __value: &[u8],
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"MutFn" => _serde::__private::Ok(__Field::__field0),
                        b"__Phantom" => _serde::__private::Ok(__Field::__field1),
                        _ => {
                            let __value = &_serde::__private::from_utf8_lossy(__value);
                            _serde::__private::Err(_serde::de::Error::unknown_variant(
                                __value, VARIANTS,
                            ))
                        }
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(
                    __deserializer: __D,
                ) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            struct __Visitor<'de, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                marker: _serde::__private::PhantomData<MyServiceReqRefMut<Codec>>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de, Codec> _serde::de::Visitor<'de> for __Visitor<'de, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                type Value = MyServiceReqRefMut<Codec>;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "enum MyServiceReqRefMut")
                }
                fn visit_enum<__A>(
                    self,
                    __data: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::EnumAccess<'de>,
                {
                    match match _serde::de::EnumAccess::variant(__data) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    } {
                        (__Field::__field0, __variant) => {
                            #[allow(non_camel_case_types)]
                            enum __Field {
                                __field0,
                                __field1,
                                __ignore,
                            }
                            struct __FieldVisitor;
                            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                                type Value = __Field;
                                fn expecting(
                                    &self,
                                    __formatter: &mut _serde::__private::Formatter,
                                ) -> _serde::__private::fmt::Result
                                {
                                    _serde::__private::Formatter::write_str(
                                        __formatter,
                                        "field identifier",
                                    )
                                }
                                fn visit_u64<__E>(
                                    self,
                                    __value: u64,
                                ) -> _serde::__private::Result<Self::Value, __E>
                                where
                                    __E: _serde::de::Error,
                                {
                                    match __value {
                                        0u64 => _serde::__private::Ok(__Field::__field0),
                                        1u64 => _serde::__private::Ok(__Field::__field1),
                                        _ => _serde::__private::Ok(__Field::__ignore),
                                    }
                                }
                                fn visit_str<__E>(
                                    self,
                                    __value: &str,
                                ) -> _serde::__private::Result<Self::Value, __E>
                                where
                                    __E: _serde::de::Error,
                                {
                                    match __value {
                                        "__reply_tx" => _serde::__private::Ok(__Field::__field0),
                                        "arg1" => _serde::__private::Ok(__Field::__field1),
                                        _ => _serde::__private::Ok(__Field::__ignore),
                                    }
                                }
                                fn visit_bytes<__E>(
                                    self,
                                    __value: &[u8],
                                ) -> _serde::__private::Result<Self::Value, __E>
                                where
                                    __E: _serde::de::Error,
                                {
                                    match __value {
                                        b"__reply_tx" => _serde::__private::Ok(__Field::__field0),
                                        b"arg1" => _serde::__private::Ok(__Field::__field1),
                                        _ => _serde::__private::Ok(__Field::__ignore),
                                    }
                                }
                            }
                            impl<'de> _serde::Deserialize<'de> for __Field {
                                #[inline]
                                fn deserialize<__D>(
                                    __deserializer: __D,
                                ) -> _serde::__private::Result<Self, __D::Error>
                                where
                                    __D: _serde::Deserializer<'de>,
                                {
                                    _serde::Deserializer::deserialize_identifier(
                                        __deserializer,
                                        __FieldVisitor,
                                    )
                                }
                            }
                            struct __Visitor<'de, Codec>
                            where
                                Codec: ::remoc::codec::Codec,
                            {
                                marker: _serde::__private::PhantomData<MyServiceReqRefMut<Codec>>,
                                lifetime: _serde::__private::PhantomData<&'de ()>,
                            }
                            impl<'de, Codec> _serde::de::Visitor<'de> for __Visitor<'de, Codec>
                            where
                                Codec: ::remoc::codec::Codec,
                            {
                                type Value = MyServiceReqRefMut<Codec>;
                                fn expecting(
                                    &self,
                                    __formatter: &mut _serde::__private::Formatter,
                                ) -> _serde::__private::fmt::Result
                                {
                                    _serde::__private::Formatter::write_str(
                                        __formatter,
                                        "struct variant MyServiceReqRefMut::MutFn",
                                    )
                                }
                                #[inline]
                                fn visit_seq<__A>(
                                    self,
                                    mut __seq: __A,
                                ) -> _serde::__private::Result<Self::Value, __A::Error>
                                where
                                    __A: _serde::de::SeqAccess<'de>,
                                {
                                    let __field0 = match match _serde::de::SeqAccess::next_element::<
                                        ::remoc::rch::oneshot::Sender<Result<(), MyError>, Codec>,
                                    >(
                                        &mut __seq
                                    ) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    } {
                                        _serde::__private::Some(__value) => __value,
                                        _serde::__private::None => {
                                            return _serde :: __private :: Err (_serde :: de :: Error :: invalid_length (0usize , & "struct variant MyServiceReqRefMut::MutFn with 2 elements")) ;
                                        }
                                    };
                                    let __field1 = match match _serde::de::SeqAccess::next_element::<
                                        Vec<String>,
                                    >(
                                        &mut __seq
                                    ) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    } {
                                        _serde::__private::Some(__value) => __value,
                                        _serde::__private::None => {
                                            return _serde :: __private :: Err (_serde :: de :: Error :: invalid_length (1usize , & "struct variant MyServiceReqRefMut::MutFn with 2 elements")) ;
                                        }
                                    };
                                    _serde::__private::Ok(MyServiceReqRefMut::MutFn {
                                        __reply_tx: __field0,
                                        arg1: __field1,
                                    })
                                }
                                #[inline]
                                fn visit_map<__A>(
                                    self,
                                    mut __map: __A,
                                ) -> _serde::__private::Result<Self::Value, __A::Error>
                                where
                                    __A: _serde::de::MapAccess<'de>,
                                {
                                    let mut __field0: _serde::__private::Option<
                                        ::remoc::rch::oneshot::Sender<Result<(), MyError>, Codec>,
                                    > = _serde::__private::None;
                                    let mut __field1: _serde::__private::Option<Vec<String>> =
                                        _serde::__private::None;
                                    while let _serde::__private::Some(__key) =
                                        match _serde::de::MapAccess::next_key::<__Field>(&mut __map)
                                        {
                                            _serde::__private::Ok(__val) => __val,
                                            _serde::__private::Err(__err) => {
                                                return _serde::__private::Err(__err);
                                            }
                                        }
                                    {
                                        match __key {
                                            __Field::__field0 => {
                                                if _serde::__private::Option::is_some(&__field0) {
                                                    return _serde :: __private :: Err (< __A :: Error as _serde :: de :: Error > :: duplicate_field ("__reply_tx")) ;
                                                }
                                                __field0 = _serde::__private::Some(
                                                    match _serde::de::MapAccess::next_value::<
                                                        ::remoc::rch::oneshot::Sender<
                                                            Result<(), MyError>,
                                                            Codec,
                                                        >,
                                                    >(
                                                        &mut __map
                                                    ) {
                                                        _serde::__private::Ok(__val) => __val,
                                                        _serde::__private::Err(__err) => {
                                                            return _serde::__private::Err(__err);
                                                        }
                                                    },
                                                );
                                            }
                                            __Field::__field1 => {
                                                if _serde::__private::Option::is_some(&__field1) {
                                                    return _serde :: __private :: Err (< __A :: Error as _serde :: de :: Error > :: duplicate_field ("arg1")) ;
                                                }
                                                __field1 = _serde::__private::Some(
                                                    match _serde::de::MapAccess::next_value::<
                                                        Vec<String>,
                                                    >(
                                                        &mut __map
                                                    ) {
                                                        _serde::__private::Ok(__val) => __val,
                                                        _serde::__private::Err(__err) => {
                                                            return _serde::__private::Err(__err);
                                                        }
                                                    },
                                                );
                                            }
                                            _ => {
                                                let _ = match _serde::de::MapAccess::next_value::<
                                                    _serde::de::IgnoredAny,
                                                >(
                                                    &mut __map
                                                ) {
                                                    _serde::__private::Ok(__val) => __val,
                                                    _serde::__private::Err(__err) => {
                                                        return _serde::__private::Err(__err);
                                                    }
                                                };
                                            }
                                        }
                                    }
                                    let __field0 = match __field0 {
                                        _serde::__private::Some(__field0) => __field0,
                                        _serde::__private::None => {
                                            match _serde::__private::de::missing_field("__reply_tx")
                                            {
                                                _serde::__private::Ok(__val) => __val,
                                                _serde::__private::Err(__err) => {
                                                    return _serde::__private::Err(__err);
                                                }
                                            }
                                        }
                                    };
                                    let __field1 = match __field1 {
                                        _serde::__private::Some(__field1) => __field1,
                                        _serde::__private::None => {
                                            match _serde::__private::de::missing_field("arg1") {
                                                _serde::__private::Ok(__val) => __val,
                                                _serde::__private::Err(__err) => {
                                                    return _serde::__private::Err(__err);
                                                }
                                            }
                                        }
                                    };
                                    _serde::__private::Ok(MyServiceReqRefMut::MutFn {
                                        __reply_tx: __field0,
                                        arg1: __field1,
                                    })
                                }
                            }
                            const FIELDS: &'static [&'static str] = &["__reply_tx", "arg1"];
                            _serde::de::VariantAccess::struct_variant(
                                __variant,
                                FIELDS,
                                __Visitor {
                                    marker: _serde::__private::PhantomData::<
                                        MyServiceReqRefMut<Codec>,
                                    >,
                                    lifetime: _serde::__private::PhantomData,
                                },
                            )
                        }
                        (__Field::__field1, __variant) => _serde::__private::Result::map(
                            _serde::de::VariantAccess::newtype_variant::<
                                ::std::marker::PhantomData<(Codec)>,
                            >(__variant),
                            MyServiceReqRefMut::__Phantom,
                        ),
                    }
                }
            }
            const VARIANTS: &'static [&'static str] = &["MutFn", "__Phantom"];
            _serde::Deserializer::deserialize_enum(
                __deserializer,
                "MyServiceReqRefMut",
                VARIANTS,
                __Visitor {
                    marker: _serde::__private::PhantomData::<MyServiceReqRefMut<Codec>>,
                    lifetime: _serde::__private::PhantomData,
                },
            )
        }
    }
};
impl<Codec> MyServiceReqRefMut<Codec>
where
    Codec: ::remoc::codec::Codec,
{
    async fn dispatch<Target>(self, target: &mut Target)
    where
        Target: MyService<Codec>,
    {
        match self {
            Self::MutFn { arg1, __reply_tx } => {
                mod util {
                    pub(super) enum Out<_0, _1> {
                        _0(_0),
                        _1(_1),
                        Disabled,
                    }
                    pub(super) type Mask = u8;
                }
                use ::tokio::macros::support::Future;
                use ::tokio::macros::support::Pin;
                use ::tokio::macros::support::Poll::{Ready, Pending};
                const BRANCHES: u32 = 2;
                let mut disabled: util::Mask = Default::default();
                if !true {
                    let mask: util::Mask = 1 << 0;
                    disabled |= mask;
                }
                if !true {
                    let mask: util::Mask = 1 << 1;
                    disabled |= mask;
                }
                let mut output = {
                    let mut futures = (__reply_tx.closed(), target.mut_fn(arg1));
                    ::tokio::macros::support::poll_fn(|cx| {
                        let mut is_pending = false;
                        let start = 0;
                        for i in 0..BRANCHES {
                            let branch;
                            #[allow(clippy::modulo_one)]
                            {
                                branch = (start + i) % BRANCHES;
                            }
                            match branch {
                                #[allow(unreachable_code)]
                                0 => {
                                    let mask = 1 << branch;
                                    if disabled & mask == mask {
                                        continue;
                                    }
                                    let (fut, ..) = &mut futures;
                                    let mut fut = unsafe { Pin::new_unchecked(fut) };
                                    let out = match fut.poll(cx) {
                                        Ready(out) => out,
                                        Pending => {
                                            is_pending = true;
                                            continue;
                                        }
                                    };
                                    disabled |= mask;
                                    #[allow(unused_variables)]
                                    #[allow(unused_mut)]
                                    match &out {
                                        () => {}
                                        _ => continue,
                                    }
                                    return Ready(util::Out::_0(out));
                                }
                                #[allow(unreachable_code)]
                                1 => {
                                    let mask = 1 << branch;
                                    if disabled & mask == mask {
                                        continue;
                                    }
                                    let (_, fut, ..) = &mut futures;
                                    let mut fut = unsafe { Pin::new_unchecked(fut) };
                                    let out = match fut.poll(cx) {
                                        Ready(out) => out,
                                        Pending => {
                                            is_pending = true;
                                            continue;
                                        }
                                    };
                                    disabled |= mask;
                                    #[allow(unused_variables)]
                                    #[allow(unused_mut)]
                                    match &out {
                                        result => {}
                                        _ => continue,
                                    }
                                    return Ready(util::Out::_1(out));
                                }
                                _ => ::core::panicking::panic_fmt(::core::fmt::Arguments::new_v1(
                                    &["internal error: entered unreachable code: "],
                                    &match (
                                        &"reaching this means there probably is an off by one bug",
                                    ) {
                                        (arg0,) => [::core::fmt::ArgumentV1::new(
                                            arg0,
                                            ::core::fmt::Display::fmt,
                                        )],
                                    },
                                )),
                            }
                        }
                        if is_pending {
                            Pending
                        } else {
                            Ready(util::Out::Disabled)
                        }
                    })
                    .await
                };
                match output {
                    util::Out::_0(()) => (),
                    util::Out::_1(result) => {
                        let _ = __reply_tx.send(result);
                    }
                    util::Out::Disabled => ::std::rt::begin_panic(
                        "all branches are disabled and there is no else branch",
                    ),
                    _ => ::core::panicking::panic_fmt(::core::fmt::Arguments::new_v1(
                        &["internal error: entered unreachable code: "],
                        &match (&"failed to match bind",) {
                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                arg0,
                                ::core::fmt::Display::fmt,
                            )],
                        },
                    )),
                }
            }
            Self::__Phantom(_) => (),
        }
    }
}
///Remote server for [#ident] taking the target object by value.
pub struct MyServiceServer<Target, Codec> {
    target: Target,
    req_rx: ::remoc::rch::mpsc::Receiver<
        ::remoc::rtc::Req<
            MyServiceReqValue<Codec>,
            MyServiceReqRef<Codec>,
            MyServiceReqRefMut<Codec>,
        >,
        Codec,
    >,
}
impl<Target, Codec> ::remoc::rtc::ServerBase for MyServiceServer<Target, Codec>
where
    Codec: ::remoc::codec::Codec,
    Target: MyService<Codec>,
{
    type Client = MyServiceClient<Codec>;
}
impl<Target, Codec> ::remoc::rtc::Server<Target, Codec> for MyServiceServer<Target, Codec>
where
    Codec: ::remoc::codec::Codec,
    Target: MyService<Codec>,
{
    fn new(target: Target, request_buffer: usize) -> (Self, Self::Client) {
        let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
        (Self { target, req_rx }, Self::Client { req_tx })
    }
    #[allow(
        clippy::let_unit_value,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds,
        clippy::used_underscore_binding
    )]
    fn serve<'async_trait>(
        self,
    ) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = Option<Target>> + 'async_trait>>
    where
        Self: 'async_trait,
    {
        Box::pin(async move {
            if let ::core::option::Option::Some(__ret) =
                ::core::option::Option::None::<Option<Target>>
            {
                return __ret;
            }
            let __self = self;
            let __ret: Option<Target> = {
                let Self {
                    mut target,
                    mut req_rx,
                } = __self;
                loop {
                    match req_rx.recv().await {
                        Ok(Some(::remoc::rtc::Req::Value(req))) => {
                            req.dispatch(target).await;
                            return None;
                        }
                        Ok(Some(::remoc::rtc::Req::Ref(req))) => {
                            req.dispatch(&target).await;
                        }
                        Ok(Some(::remoc::rtc::Req::RefMut(req))) => {
                            req.dispatch(&mut target).await;
                        }
                        Ok(None) => return Some(target),
                        Err(err) => ::remoc::rtc::receiving_request_failed(err),
                    }
                }
            };
            #[allow(unreachable_code)]
            __ret
        })
    }
}
///Remote server for [#ident] taking the target object by mutable reference.
pub struct MyServiceServerRefMut<'target, Target, Codec> {
    target: &'target mut Target,
    req_rx: ::remoc::rch::mpsc::Receiver<
        ::remoc::rtc::Req<
            MyServiceReqValue<Codec>,
            MyServiceReqRef<Codec>,
            MyServiceReqRefMut<Codec>,
        >,
        Codec,
    >,
}
impl<'target, Target, Codec> ::remoc::rtc::ServerBase
    for MyServiceServerRefMut<'target, Target, Codec>
where
    Codec: ::remoc::codec::Codec,
    Target: MyService<Codec>,
{
    type Client = MyServiceClient<Codec>;
}
impl<'target, Target, Codec> ::remoc::rtc::ServerRefMut<'target, Target, Codec>
    for MyServiceServerRefMut<'target, Target, Codec>
where
    Codec: ::remoc::codec::Codec,
    Target: MyService<Codec>,
{
    fn new(target: &'target mut Target, request_buffer: usize) -> (Self, Self::Client) {
        let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
        (Self { target, req_rx }, Self::Client { req_tx })
    }
    #[allow(
        clippy::let_unit_value,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds,
        clippy::used_underscore_binding
    )]
    fn serve<'async_trait>(
        self,
    ) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = ()> + 'async_trait>>
    where
        Self: 'async_trait,
    {
        Box::pin(async move {
            let __self = self;
            let _: () = {
                let Self { target, mut req_rx } = __self;
                loop {
                    match req_rx.recv().await {
                        Ok(Some(::remoc::rtc::Req::Ref(req))) => {
                            req.dispatch(target).await;
                        }
                        Ok(Some(::remoc::rtc::Req::RefMut(req))) => {
                            req.dispatch(target).await;
                        }
                        Ok(Some(_)) => (),
                        Ok(None) => break,
                        Err(err) => ::remoc::rtc::receiving_request_failed(err),
                    }
                }
            };
        })
    }
}
///Remote server for [#ident] taking the target object by shared mutable reference.
pub struct MyServiceServerSharedMut<Target, Codec> {
    target: ::std::sync::Arc<::remoc::rtc::LocalRwLock<Target>>,
    req_rx: ::remoc::rch::mpsc::Receiver<
        ::remoc::rtc::Req<
            MyServiceReqValue<Codec>,
            MyServiceReqRef<Codec>,
            MyServiceReqRefMut<Codec>,
        >,
        Codec,
    >,
}
impl<Target, Codec> ::remoc::rtc::ServerBase for MyServiceServerSharedMut<Target, Codec>
where
    Codec: ::remoc::codec::Codec,
    Target: MyService<Codec>,
    Target: ::std::marker::Send + ::std::marker::Sync + 'static,
{
    type Client = MyServiceClient<Codec>;
}
impl<Target, Codec> ::remoc::rtc::ServerSharedMut<Target, Codec>
    for MyServiceServerSharedMut<Target, Codec>
where
    Codec: ::remoc::codec::Codec,
    Target: MyService<Codec>,
    Target: ::std::marker::Send + ::std::marker::Sync + 'static,
{
    fn new(
        target: ::std::sync::Arc<::remoc::rtc::LocalRwLock<Target>>,
        request_buffer: usize,
    ) -> (Self, Self::Client) {
        let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
        (Self { target, req_rx }, Self::Client { req_tx })
    }
    #[allow(
        clippy::let_unit_value,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds,
        clippy::used_underscore_binding
    )]
    fn serve<'async_trait>(
        self,
        spawn: bool,
    ) -> ::core::pin::Pin<
        Box<dyn ::core::future::Future<Output = ()> + ::core::marker::Send + 'async_trait>,
    >
    where
        Self: 'async_trait,
    {
        Box::pin(async move {
            let __self = self;
            let spawn = spawn;
            let _: () = {
                let Self { target, mut req_rx } = __self;
                loop {
                    match req_rx.recv().await {
                        Ok(Some(::remoc::rtc::Req::Ref(req))) => {
                            if spawn {
                                let target = target.clone().read_owned().await;
                                ::remoc::rtc::spawn(async move {
                                    req.dispatch(&*target).await;
                                });
                            } else {
                                let target = target.read().await;
                                req.dispatch(&*target).await;
                            }
                        }
                        Ok(Some(::remoc::rtc::Req::RefMut(req))) => {
                            let mut target = target.write().await;
                            req.dispatch(&mut *target).await;
                        }
                        Ok(Some(_)) => (),
                        Ok(None) => break,
                        Err(err) => ::remoc::rtc::receiving_request_failed(err),
                    }
                }
            };
        })
    }
}
#[serde(bound(serialize = "Codec: ::remoc::codec::Codec"))]
#[serde(bound(deserialize = "Codec: ::remoc::codec::Codec"))]
pub struct MyServiceClient<Codec> {
    req_tx: ::remoc::rch::mpsc::Sender<
        ::remoc::rtc::Req<
            MyServiceReqValue<Codec>,
            MyServiceReqRef<Codec>,
            MyServiceReqRefMut<Codec>,
        >,
        Codec,
    >,
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<Codec> _serde::Serialize for MyServiceClient<Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn serialize<__S>(
            &self,
            __serializer: __S,
        ) -> _serde::__private::Result<__S::Ok, __S::Error>
        where
            __S: _serde::Serializer,
        {
            let mut __serde_state = match _serde::Serializer::serialize_struct(
                __serializer,
                "MyServiceClient",
                false as usize + 1,
            ) {
                _serde::__private::Ok(__val) => __val,
                _serde::__private::Err(__err) => {
                    return _serde::__private::Err(__err);
                }
            };
            match _serde::ser::SerializeStruct::serialize_field(
                &mut __serde_state,
                "req_tx",
                &self.req_tx,
            ) {
                _serde::__private::Ok(__val) => __val,
                _serde::__private::Err(__err) => {
                    return _serde::__private::Err(__err);
                }
            };
            _serde::ser::SerializeStruct::end(__serde_state)
        }
    }
};
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<'de, Codec> _serde::Deserialize<'de> for MyServiceClient<Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
        where
            __D: _serde::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            enum __Field {
                __field0,
                __ignore,
            }
            struct __FieldVisitor;
            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                type Value = __Field;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "field identifier")
                }
                fn visit_u64<__E>(self, __value: u64) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        0u64 => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
                fn visit_str<__E>(
                    self,
                    __value: &str,
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "req_tx" => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
                fn visit_bytes<__E>(
                    self,
                    __value: &[u8],
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"req_tx" => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(
                    __deserializer: __D,
                ) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            struct __Visitor<'de, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                marker: _serde::__private::PhantomData<MyServiceClient<Codec>>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de, Codec> _serde::de::Visitor<'de> for __Visitor<'de, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                type Value = MyServiceClient<Codec>;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "struct MyServiceClient")
                }
                #[inline]
                fn visit_seq<__A>(
                    self,
                    mut __seq: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::SeqAccess<'de>,
                {
                    let __field0 = match match _serde::de::SeqAccess::next_element::<
                        ::remoc::rch::mpsc::Sender<
                            ::remoc::rtc::Req<
                                MyServiceReqValue<Codec>,
                                MyServiceReqRef<Codec>,
                                MyServiceReqRefMut<Codec>,
                            >,
                            Codec,
                        >,
                    >(&mut __seq)
                    {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    } {
                        _serde::__private::Some(__value) => __value,
                        _serde::__private::None => {
                            return _serde::__private::Err(_serde::de::Error::invalid_length(
                                0usize,
                                &"struct MyServiceClient with 1 element",
                            ));
                        }
                    };
                    _serde::__private::Ok(MyServiceClient { req_tx: __field0 })
                }
                #[inline]
                fn visit_map<__A>(
                    self,
                    mut __map: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::MapAccess<'de>,
                {
                    let mut __field0: _serde::__private::Option<
                        ::remoc::rch::mpsc::Sender<
                            ::remoc::rtc::Req<
                                MyServiceReqValue<Codec>,
                                MyServiceReqRef<Codec>,
                                MyServiceReqRefMut<Codec>,
                            >,
                            Codec,
                        >,
                    > = _serde::__private::None;
                    while let _serde::__private::Some(__key) =
                        match _serde::de::MapAccess::next_key::<__Field>(&mut __map) {
                            _serde::__private::Ok(__val) => __val,
                            _serde::__private::Err(__err) => {
                                return _serde::__private::Err(__err);
                            }
                        }
                    {
                        match __key {
                            __Field::__field0 => {
                                if _serde::__private::Option::is_some(&__field0) {
                                    return _serde::__private::Err(
                                        <__A::Error as _serde::de::Error>::duplicate_field(
                                            "req_tx",
                                        ),
                                    );
                                }
                                __field0 = _serde::__private::Some(
                                    match _serde::de::MapAccess::next_value::<
                                        ::remoc::rch::mpsc::Sender<
                                            ::remoc::rtc::Req<
                                                MyServiceReqValue<Codec>,
                                                MyServiceReqRef<Codec>,
                                                MyServiceReqRefMut<Codec>,
                                            >,
                                            Codec,
                                        >,
                                    >(&mut __map)
                                    {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    },
                                );
                            }
                            _ => {
                                let _ = match _serde::de::MapAccess::next_value::<
                                    _serde::de::IgnoredAny,
                                >(&mut __map)
                                {
                                    _serde::__private::Ok(__val) => __val,
                                    _serde::__private::Err(__err) => {
                                        return _serde::__private::Err(__err);
                                    }
                                };
                            }
                        }
                    }
                    let __field0 = match __field0 {
                        _serde::__private::Some(__field0) => __field0,
                        _serde::__private::None => {
                            match _serde::__private::de::missing_field("req_tx") {
                                _serde::__private::Ok(__val) => __val,
                                _serde::__private::Err(__err) => {
                                    return _serde::__private::Err(__err);
                                }
                            }
                        }
                    };
                    _serde::__private::Ok(MyServiceClient { req_tx: __field0 })
                }
            }
            const FIELDS: &'static [&'static str] = &["req_tx"];
            _serde::Deserializer::deserialize_struct(
                __deserializer,
                "MyServiceClient",
                FIELDS,
                __Visitor {
                    marker: _serde::__private::PhantomData::<MyServiceClient<Codec>>,
                    lifetime: _serde::__private::PhantomData,
                },
            )
        }
    }
};
impl<Codec> MyService<Codec> for MyServiceClient<Codec>
where
    Codec: ::remoc::codec::Codec,
{
    #[allow(
        clippy::let_unit_value,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds,
        clippy::used_underscore_binding
    )]
    fn const_fn<'life0, 'async_trait>(
        &'life0 self,
        arg1: String,
        arg2: u16,
        arg3: remoc::rch::mpsc::Sender<String, Codec>,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<u32, MyError>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            if let ::core::option::Option::Some(__ret) =
                ::core::option::Option::None::<Result<u32, MyError>>
            {
                return __ret;
            }
            let __self = self;
            let arg1 = arg1;
            let arg2 = arg2;
            let arg3 = arg3;
            let __ret: Result<u32, MyError> = {
                let (reply_tx, reply_rx) = ::remoc::rch::oneshot::channel();
                let req_value = MyServiceReqRef::ConstFn {
                    __reply_tx: reply_tx,
                    arg1,
                    arg2,
                    arg3,
                };
                let req = ::remoc::rtc::Req::Ref(req_value);
                __self
                    .req_tx
                    .send(req)
                    .await
                    .map_err(::remoc::rtc::CallError::from)?;
                let reply = reply_rx.await.map_err(::remoc::rtc::CallError::from)?;
                reply
            };
            #[allow(unreachable_code)]
            __ret
        })
    }
    #[allow(
        clippy::let_unit_value,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds,
        clippy::used_underscore_binding
    )]
    fn mut_fn<'life0, 'async_trait>(
        &'life0 mut self,
        arg1: Vec<String>,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<(), MyError>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            if let ::core::option::Option::Some(__ret) =
                ::core::option::Option::None::<Result<(), MyError>>
            {
                return __ret;
            }
            let mut __self = self;
            let arg1 = arg1;
            let __ret: Result<(), MyError> = {
                let (reply_tx, reply_rx) = ::remoc::rch::oneshot::channel();
                let req_value = MyServiceReqRefMut::MutFn {
                    __reply_tx: reply_tx,
                    arg1,
                };
                let req = ::remoc::rtc::Req::RefMut(req_value);
                __self
                    .req_tx
                    .send(req)
                    .await
                    .map_err(::remoc::rtc::CallError::from)?;
                let reply = reply_rx.await.map_err(::remoc::rtc::CallError::from)?;
                reply
            };
            #[allow(unreachable_code)]
            __ret
        })
    }
}
impl<Codec> ::std::fmt::Debug for MyServiceClient<Codec> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.write_fmt(::core::fmt::Arguments::new_v1(
            &["#client_ident"],
            &match () {
                () => [],
            },
        ))
    }
}
impl<Codec> ::std::ops::Drop for MyServiceClient<Codec> {
    fn drop(&mut self) {}
}
pub struct MyObject {
    field1: String,
}
impl<Codec> MyService<Codec> for MyObject
where
    Codec: remoc::codec::Codec,
{
    #[allow(
        clippy::let_unit_value,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds,
        clippy::used_underscore_binding
    )]
    fn const_fn<'life0, 'async_trait>(
        &'life0 self,
        arg1: String,
        arg2: u16,
        arg3: remoc::rch::mpsc::Sender<String, Codec>,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<u32, MyError>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            if let ::core::option::Option::Some(__ret) =
                ::core::option::Option::None::<Result<u32, MyError>>
            {
                return __ret;
            }
            let __self = self;
            let arg1 = arg1;
            let arg2 = arg2;
            let arg3 = arg3;
            let __ret: Result<u32, MyError> = {
                {
                    ::std::io::_print(::core::fmt::Arguments::new_v1(
                        &["arg1: ", ", arg2: ", "\n"],
                        &match (&arg1, &arg2) {
                            (arg0, arg1) => [
                                ::core::fmt::ArgumentV1::new(arg0, ::core::fmt::Display::fmt),
                                ::core::fmt::ArgumentV1::new(arg1, ::core::fmt::Display::fmt),
                            ],
                        },
                    ));
                };
                arg3.send("Hallo".to_string()).await.unwrap();
                Ok(123)
            };
            #[allow(unreachable_code)]
            __ret
        })
    }
    #[allow(
        clippy::let_unit_value,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds,
        clippy::used_underscore_binding
    )]
    fn mut_fn<'life0, 'async_trait>(
        &'life0 mut self,
        arg1: Vec<String>,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<(), MyError>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            if let ::core::option::Option::Some(__ret) =
                ::core::option::Option::None::<Result<(), MyError>>
            {
                return __ret;
            }
            let mut __self = self;
            let arg1 = arg1;
            let __ret: Result<(), MyError> = {
                __self.field1 = arg1.join(",");
                Err(MyError::Error1)
            };
            #[allow(unreachable_code)]
            __ret
        })
    }
}
pub async fn do_test() {
    let obj = MyObject {
        field1: String::new(),
    };
}
pub trait MyGenericService<T, Codec>
where
    T: ::remoc::RemoteSend,
{
    /// Mut fn docs.
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    fn mut_fn<'life0, 'async_trait>(
        &'life0 mut self,
        arg1: T,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<(), MyError>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait;
}
#[serde(bound(serialize = "Codec: ::remoc::codec::Codec"))]
#[serde(bound(deserialize = "Codec: ::remoc::codec::Codec"))]
enum MyGenericServiceReqValue<T, Codec> {
    __Phantom(::std::marker::PhantomData<(T, Codec)>),
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<T, Codec> _serde::Serialize for MyGenericServiceReqValue<T, Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn serialize<__S>(
            &self,
            __serializer: __S,
        ) -> _serde::__private::Result<__S::Ok, __S::Error>
        where
            __S: _serde::Serializer,
        {
            match *self {
                MyGenericServiceReqValue::__Phantom(ref __field0) => {
                    _serde::Serializer::serialize_newtype_variant(
                        __serializer,
                        "MyGenericServiceReqValue",
                        0u32,
                        "__Phantom",
                        __field0,
                    )
                }
            }
        }
    }
};
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<'de, T, Codec> _serde::Deserialize<'de> for MyGenericServiceReqValue<T, Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
        where
            __D: _serde::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            enum __Field {
                __field0,
            }
            struct __FieldVisitor;
            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                type Value = __Field;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "variant identifier")
                }
                fn visit_u64<__E>(self, __value: u64) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        0u64 => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Err(_serde::de::Error::invalid_value(
                            _serde::de::Unexpected::Unsigned(__value),
                            &"variant index 0 <= i < 1",
                        )),
                    }
                }
                fn visit_str<__E>(
                    self,
                    __value: &str,
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "__Phantom" => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Err(_serde::de::Error::unknown_variant(
                            __value, VARIANTS,
                        )),
                    }
                }
                fn visit_bytes<__E>(
                    self,
                    __value: &[u8],
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"__Phantom" => _serde::__private::Ok(__Field::__field0),
                        _ => {
                            let __value = &_serde::__private::from_utf8_lossy(__value);
                            _serde::__private::Err(_serde::de::Error::unknown_variant(
                                __value, VARIANTS,
                            ))
                        }
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(
                    __deserializer: __D,
                ) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            struct __Visitor<'de, T, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                marker: _serde::__private::PhantomData<MyGenericServiceReqValue<T, Codec>>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de, T, Codec> _serde::de::Visitor<'de> for __Visitor<'de, T, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                type Value = MyGenericServiceReqValue<T, Codec>;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(
                        __formatter,
                        "enum MyGenericServiceReqValue",
                    )
                }
                fn visit_enum<__A>(
                    self,
                    __data: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::EnumAccess<'de>,
                {
                    match match _serde::de::EnumAccess::variant(__data) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    } {
                        (__Field::__field0, __variant) => _serde::__private::Result::map(
                            _serde::de::VariantAccess::newtype_variant::<
                                ::std::marker::PhantomData<(T, Codec)>,
                            >(__variant),
                            MyGenericServiceReqValue::__Phantom,
                        ),
                    }
                }
            }
            const VARIANTS: &'static [&'static str] = &["__Phantom"];
            _serde::Deserializer::deserialize_enum(
                __deserializer,
                "MyGenericServiceReqValue",
                VARIANTS,
                __Visitor {
                    marker: _serde::__private::PhantomData::<MyGenericServiceReqValue<T, Codec>>,
                    lifetime: _serde::__private::PhantomData,
                },
            )
        }
    }
};
impl<T, Codec> MyGenericServiceReqValue<T, Codec>
where
    T: ::remoc::RemoteSend,
    Codec: ::remoc::codec::Codec,
{
    async fn dispatch<Target>(self, target: Target)
    where
        Target: MyGenericService<T, Codec>,
    {
        match self {
            Self::__Phantom(_) => (),
        }
    }
}
#[serde(bound(serialize = "Codec: ::remoc::codec::Codec"))]
#[serde(bound(deserialize = "Codec: ::remoc::codec::Codec"))]
enum MyGenericServiceReqRef<T, Codec> {
    __Phantom(::std::marker::PhantomData<(T, Codec)>),
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<T, Codec> _serde::Serialize for MyGenericServiceReqRef<T, Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn serialize<__S>(
            &self,
            __serializer: __S,
        ) -> _serde::__private::Result<__S::Ok, __S::Error>
        where
            __S: _serde::Serializer,
        {
            match *self {
                MyGenericServiceReqRef::__Phantom(ref __field0) => {
                    _serde::Serializer::serialize_newtype_variant(
                        __serializer,
                        "MyGenericServiceReqRef",
                        0u32,
                        "__Phantom",
                        __field0,
                    )
                }
            }
        }
    }
};
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<'de, T, Codec> _serde::Deserialize<'de> for MyGenericServiceReqRef<T, Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
        where
            __D: _serde::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            enum __Field {
                __field0,
            }
            struct __FieldVisitor;
            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                type Value = __Field;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "variant identifier")
                }
                fn visit_u64<__E>(self, __value: u64) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        0u64 => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Err(_serde::de::Error::invalid_value(
                            _serde::de::Unexpected::Unsigned(__value),
                            &"variant index 0 <= i < 1",
                        )),
                    }
                }
                fn visit_str<__E>(
                    self,
                    __value: &str,
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "__Phantom" => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Err(_serde::de::Error::unknown_variant(
                            __value, VARIANTS,
                        )),
                    }
                }
                fn visit_bytes<__E>(
                    self,
                    __value: &[u8],
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"__Phantom" => _serde::__private::Ok(__Field::__field0),
                        _ => {
                            let __value = &_serde::__private::from_utf8_lossy(__value);
                            _serde::__private::Err(_serde::de::Error::unknown_variant(
                                __value, VARIANTS,
                            ))
                        }
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(
                    __deserializer: __D,
                ) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            struct __Visitor<'de, T, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                marker: _serde::__private::PhantomData<MyGenericServiceReqRef<T, Codec>>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de, T, Codec> _serde::de::Visitor<'de> for __Visitor<'de, T, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                type Value = MyGenericServiceReqRef<T, Codec>;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(
                        __formatter,
                        "enum MyGenericServiceReqRef",
                    )
                }
                fn visit_enum<__A>(
                    self,
                    __data: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::EnumAccess<'de>,
                {
                    match match _serde::de::EnumAccess::variant(__data) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    } {
                        (__Field::__field0, __variant) => _serde::__private::Result::map(
                            _serde::de::VariantAccess::newtype_variant::<
                                ::std::marker::PhantomData<(T, Codec)>,
                            >(__variant),
                            MyGenericServiceReqRef::__Phantom,
                        ),
                    }
                }
            }
            const VARIANTS: &'static [&'static str] = &["__Phantom"];
            _serde::Deserializer::deserialize_enum(
                __deserializer,
                "MyGenericServiceReqRef",
                VARIANTS,
                __Visitor {
                    marker: _serde::__private::PhantomData::<MyGenericServiceReqRef<T, Codec>>,
                    lifetime: _serde::__private::PhantomData,
                },
            )
        }
    }
};
impl<T, Codec> MyGenericServiceReqRef<T, Codec>
where
    T: ::remoc::RemoteSend,
    Codec: ::remoc::codec::Codec,
{
    async fn dispatch<Target>(self, target: &Target)
    where
        Target: MyGenericService<T, Codec>,
    {
        match self {
            Self::__Phantom(_) => (),
        }
    }
}
#[serde(bound(serialize = "Codec: ::remoc::codec::Codec"))]
#[serde(bound(deserialize = "Codec: ::remoc::codec::Codec"))]
enum MyGenericServiceReqRefMut<T, Codec> {
    MutFn {
        __reply_tx: ::remoc::rch::oneshot::Sender<Result<(), MyError>, Codec>,
        arg1: T,
    },
    __Phantom(::std::marker::PhantomData<(T, Codec)>),
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<T, Codec> _serde::Serialize for MyGenericServiceReqRefMut<T, Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn serialize<__S>(
            &self,
            __serializer: __S,
        ) -> _serde::__private::Result<__S::Ok, __S::Error>
        where
            __S: _serde::Serializer,
        {
            match *self {
                MyGenericServiceReqRefMut::MutFn {
                    ref __reply_tx,
                    ref arg1,
                } => {
                    let mut __serde_state = match _serde::Serializer::serialize_struct_variant(
                        __serializer,
                        "MyGenericServiceReqRefMut",
                        0u32,
                        "MutFn",
                        0 + 1 + 1,
                    ) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    };
                    match _serde::ser::SerializeStructVariant::serialize_field(
                        &mut __serde_state,
                        "__reply_tx",
                        __reply_tx,
                    ) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    };
                    match _serde::ser::SerializeStructVariant::serialize_field(
                        &mut __serde_state,
                        "arg1",
                        arg1,
                    ) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    };
                    _serde::ser::SerializeStructVariant::end(__serde_state)
                }
                MyGenericServiceReqRefMut::__Phantom(ref __field0) => {
                    _serde::Serializer::serialize_newtype_variant(
                        __serializer,
                        "MyGenericServiceReqRefMut",
                        1u32,
                        "__Phantom",
                        __field0,
                    )
                }
            }
        }
    }
};
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<'de, T, Codec> _serde::Deserialize<'de> for MyGenericServiceReqRefMut<T, Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
        where
            __D: _serde::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            enum __Field {
                __field0,
                __field1,
            }
            struct __FieldVisitor;
            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                type Value = __Field;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "variant identifier")
                }
                fn visit_u64<__E>(self, __value: u64) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        0u64 => _serde::__private::Ok(__Field::__field0),
                        1u64 => _serde::__private::Ok(__Field::__field1),
                        _ => _serde::__private::Err(_serde::de::Error::invalid_value(
                            _serde::de::Unexpected::Unsigned(__value),
                            &"variant index 0 <= i < 2",
                        )),
                    }
                }
                fn visit_str<__E>(
                    self,
                    __value: &str,
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "MutFn" => _serde::__private::Ok(__Field::__field0),
                        "__Phantom" => _serde::__private::Ok(__Field::__field1),
                        _ => _serde::__private::Err(_serde::de::Error::unknown_variant(
                            __value, VARIANTS,
                        )),
                    }
                }
                fn visit_bytes<__E>(
                    self,
                    __value: &[u8],
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"MutFn" => _serde::__private::Ok(__Field::__field0),
                        b"__Phantom" => _serde::__private::Ok(__Field::__field1),
                        _ => {
                            let __value = &_serde::__private::from_utf8_lossy(__value);
                            _serde::__private::Err(_serde::de::Error::unknown_variant(
                                __value, VARIANTS,
                            ))
                        }
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(
                    __deserializer: __D,
                ) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            struct __Visitor<'de, T, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                marker: _serde::__private::PhantomData<MyGenericServiceReqRefMut<T, Codec>>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de, T, Codec> _serde::de::Visitor<'de> for __Visitor<'de, T, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                type Value = MyGenericServiceReqRefMut<T, Codec>;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(
                        __formatter,
                        "enum MyGenericServiceReqRefMut",
                    )
                }
                fn visit_enum<__A>(
                    self,
                    __data: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::EnumAccess<'de>,
                {
                    match match _serde::de::EnumAccess::variant(__data) {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    } {
                        (__Field::__field0, __variant) => {
                            #[allow(non_camel_case_types)]
                            enum __Field {
                                __field0,
                                __field1,
                                __ignore,
                            }
                            struct __FieldVisitor;
                            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                                type Value = __Field;
                                fn expecting(
                                    &self,
                                    __formatter: &mut _serde::__private::Formatter,
                                ) -> _serde::__private::fmt::Result
                                {
                                    _serde::__private::Formatter::write_str(
                                        __formatter,
                                        "field identifier",
                                    )
                                }
                                fn visit_u64<__E>(
                                    self,
                                    __value: u64,
                                ) -> _serde::__private::Result<Self::Value, __E>
                                where
                                    __E: _serde::de::Error,
                                {
                                    match __value {
                                        0u64 => _serde::__private::Ok(__Field::__field0),
                                        1u64 => _serde::__private::Ok(__Field::__field1),
                                        _ => _serde::__private::Ok(__Field::__ignore),
                                    }
                                }
                                fn visit_str<__E>(
                                    self,
                                    __value: &str,
                                ) -> _serde::__private::Result<Self::Value, __E>
                                where
                                    __E: _serde::de::Error,
                                {
                                    match __value {
                                        "__reply_tx" => _serde::__private::Ok(__Field::__field0),
                                        "arg1" => _serde::__private::Ok(__Field::__field1),
                                        _ => _serde::__private::Ok(__Field::__ignore),
                                    }
                                }
                                fn visit_bytes<__E>(
                                    self,
                                    __value: &[u8],
                                ) -> _serde::__private::Result<Self::Value, __E>
                                where
                                    __E: _serde::de::Error,
                                {
                                    match __value {
                                        b"__reply_tx" => _serde::__private::Ok(__Field::__field0),
                                        b"arg1" => _serde::__private::Ok(__Field::__field1),
                                        _ => _serde::__private::Ok(__Field::__ignore),
                                    }
                                }
                            }
                            impl<'de> _serde::Deserialize<'de> for __Field {
                                #[inline]
                                fn deserialize<__D>(
                                    __deserializer: __D,
                                ) -> _serde::__private::Result<Self, __D::Error>
                                where
                                    __D: _serde::Deserializer<'de>,
                                {
                                    _serde::Deserializer::deserialize_identifier(
                                        __deserializer,
                                        __FieldVisitor,
                                    )
                                }
                            }
                            struct __Visitor<'de, T, Codec>
                            where
                                Codec: ::remoc::codec::Codec,
                            {
                                marker: _serde::__private::PhantomData<
                                    MyGenericServiceReqRefMut<T, Codec>,
                                >,
                                lifetime: _serde::__private::PhantomData<&'de ()>,
                            }
                            impl<'de, T, Codec> _serde::de::Visitor<'de> for __Visitor<'de, T, Codec>
                            where
                                Codec: ::remoc::codec::Codec,
                            {
                                type Value = MyGenericServiceReqRefMut<T, Codec>;
                                fn expecting(
                                    &self,
                                    __formatter: &mut _serde::__private::Formatter,
                                ) -> _serde::__private::fmt::Result
                                {
                                    _serde::__private::Formatter::write_str(
                                        __formatter,
                                        "struct variant MyGenericServiceReqRefMut::MutFn",
                                    )
                                }
                                #[inline]
                                fn visit_seq<__A>(
                                    self,
                                    mut __seq: __A,
                                ) -> _serde::__private::Result<Self::Value, __A::Error>
                                where
                                    __A: _serde::de::SeqAccess<'de>,
                                {
                                    let __field0 = match match _serde::de::SeqAccess::next_element::<
                                        ::remoc::rch::oneshot::Sender<Result<(), MyError>, Codec>,
                                    >(
                                        &mut __seq
                                    ) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    } {
                                        _serde::__private::Some(__value) => __value,
                                        _serde::__private::None => {
                                            return _serde :: __private :: Err (_serde :: de :: Error :: invalid_length (0usize , & "struct variant MyGenericServiceReqRefMut::MutFn with 2 elements")) ;
                                        }
                                    };
                                    let __field1 = match match _serde::de::SeqAccess::next_element::<
                                        T,
                                    >(
                                        &mut __seq
                                    ) {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    } {
                                        _serde::__private::Some(__value) => __value,
                                        _serde::__private::None => {
                                            return _serde :: __private :: Err (_serde :: de :: Error :: invalid_length (1usize , & "struct variant MyGenericServiceReqRefMut::MutFn with 2 elements")) ;
                                        }
                                    };
                                    _serde::__private::Ok(MyGenericServiceReqRefMut::MutFn {
                                        __reply_tx: __field0,
                                        arg1: __field1,
                                    })
                                }
                                #[inline]
                                fn visit_map<__A>(
                                    self,
                                    mut __map: __A,
                                ) -> _serde::__private::Result<Self::Value, __A::Error>
                                where
                                    __A: _serde::de::MapAccess<'de>,
                                {
                                    let mut __field0: _serde::__private::Option<
                                        ::remoc::rch::oneshot::Sender<Result<(), MyError>, Codec>,
                                    > = _serde::__private::None;
                                    let mut __field1: _serde::__private::Option<T> =
                                        _serde::__private::None;
                                    while let _serde::__private::Some(__key) =
                                        match _serde::de::MapAccess::next_key::<__Field>(&mut __map)
                                        {
                                            _serde::__private::Ok(__val) => __val,
                                            _serde::__private::Err(__err) => {
                                                return _serde::__private::Err(__err);
                                            }
                                        }
                                    {
                                        match __key {
                                            __Field::__field0 => {
                                                if _serde::__private::Option::is_some(&__field0) {
                                                    return _serde :: __private :: Err (< __A :: Error as _serde :: de :: Error > :: duplicate_field ("__reply_tx")) ;
                                                }
                                                __field0 = _serde::__private::Some(
                                                    match _serde::de::MapAccess::next_value::<
                                                        ::remoc::rch::oneshot::Sender<
                                                            Result<(), MyError>,
                                                            Codec,
                                                        >,
                                                    >(
                                                        &mut __map
                                                    ) {
                                                        _serde::__private::Ok(__val) => __val,
                                                        _serde::__private::Err(__err) => {
                                                            return _serde::__private::Err(__err);
                                                        }
                                                    },
                                                );
                                            }
                                            __Field::__field1 => {
                                                if _serde::__private::Option::is_some(&__field1) {
                                                    return _serde :: __private :: Err (< __A :: Error as _serde :: de :: Error > :: duplicate_field ("arg1")) ;
                                                }
                                                __field1 = _serde::__private::Some(
                                                    match _serde::de::MapAccess::next_value::<T>(
                                                        &mut __map,
                                                    ) {
                                                        _serde::__private::Ok(__val) => __val,
                                                        _serde::__private::Err(__err) => {
                                                            return _serde::__private::Err(__err);
                                                        }
                                                    },
                                                );
                                            }
                                            _ => {
                                                let _ = match _serde::de::MapAccess::next_value::<
                                                    _serde::de::IgnoredAny,
                                                >(
                                                    &mut __map
                                                ) {
                                                    _serde::__private::Ok(__val) => __val,
                                                    _serde::__private::Err(__err) => {
                                                        return _serde::__private::Err(__err);
                                                    }
                                                };
                                            }
                                        }
                                    }
                                    let __field0 = match __field0 {
                                        _serde::__private::Some(__field0) => __field0,
                                        _serde::__private::None => {
                                            match _serde::__private::de::missing_field("__reply_tx")
                                            {
                                                _serde::__private::Ok(__val) => __val,
                                                _serde::__private::Err(__err) => {
                                                    return _serde::__private::Err(__err);
                                                }
                                            }
                                        }
                                    };
                                    let __field1 = match __field1 {
                                        _serde::__private::Some(__field1) => __field1,
                                        _serde::__private::None => {
                                            match _serde::__private::de::missing_field("arg1") {
                                                _serde::__private::Ok(__val) => __val,
                                                _serde::__private::Err(__err) => {
                                                    return _serde::__private::Err(__err);
                                                }
                                            }
                                        }
                                    };
                                    _serde::__private::Ok(MyGenericServiceReqRefMut::MutFn {
                                        __reply_tx: __field0,
                                        arg1: __field1,
                                    })
                                }
                            }
                            const FIELDS: &'static [&'static str] = &["__reply_tx", "arg1"];
                            _serde::de::VariantAccess::struct_variant(
                                __variant,
                                FIELDS,
                                __Visitor {
                                    marker: _serde::__private::PhantomData::<
                                        MyGenericServiceReqRefMut<T, Codec>,
                                    >,
                                    lifetime: _serde::__private::PhantomData,
                                },
                            )
                        }
                        (__Field::__field1, __variant) => _serde::__private::Result::map(
                            _serde::de::VariantAccess::newtype_variant::<
                                ::std::marker::PhantomData<(T, Codec)>,
                            >(__variant),
                            MyGenericServiceReqRefMut::__Phantom,
                        ),
                    }
                }
            }
            const VARIANTS: &'static [&'static str] = &["MutFn", "__Phantom"];
            _serde::Deserializer::deserialize_enum(
                __deserializer,
                "MyGenericServiceReqRefMut",
                VARIANTS,
                __Visitor {
                    marker: _serde::__private::PhantomData::<MyGenericServiceReqRefMut<T, Codec>>,
                    lifetime: _serde::__private::PhantomData,
                },
            )
        }
    }
};
impl<T, Codec> MyGenericServiceReqRefMut<T, Codec>
where
    T: ::remoc::RemoteSend,
    Codec: ::remoc::codec::Codec,
{
    async fn dispatch<Target>(self, target: &mut Target)
    where
        Target: MyGenericService<T, Codec>,
    {
        match self {
            Self::MutFn { arg1, __reply_tx } => {
                mod util {
                    pub(super) enum Out<_0, _1> {
                        _0(_0),
                        _1(_1),
                        Disabled,
                    }
                    pub(super) type Mask = u8;
                }
                use ::tokio::macros::support::Future;
                use ::tokio::macros::support::Pin;
                use ::tokio::macros::support::Poll::{Ready, Pending};
                const BRANCHES: u32 = 2;
                let mut disabled: util::Mask = Default::default();
                if !true {
                    let mask: util::Mask = 1 << 0;
                    disabled |= mask;
                }
                if !true {
                    let mask: util::Mask = 1 << 1;
                    disabled |= mask;
                }
                let mut output = {
                    let mut futures = (__reply_tx.closed(), target.mut_fn(arg1));
                    ::tokio::macros::support::poll_fn(|cx| {
                        let mut is_pending = false;
                        let start = 0;
                        for i in 0..BRANCHES {
                            let branch;
                            #[allow(clippy::modulo_one)]
                            {
                                branch = (start + i) % BRANCHES;
                            }
                            match branch {
                                #[allow(unreachable_code)]
                                0 => {
                                    let mask = 1 << branch;
                                    if disabled & mask == mask {
                                        continue;
                                    }
                                    let (fut, ..) = &mut futures;
                                    let mut fut = unsafe { Pin::new_unchecked(fut) };
                                    let out = match fut.poll(cx) {
                                        Ready(out) => out,
                                        Pending => {
                                            is_pending = true;
                                            continue;
                                        }
                                    };
                                    disabled |= mask;
                                    #[allow(unused_variables)]
                                    #[allow(unused_mut)]
                                    match &out {
                                        () => {}
                                        _ => continue,
                                    }
                                    return Ready(util::Out::_0(out));
                                }
                                #[allow(unreachable_code)]
                                1 => {
                                    let mask = 1 << branch;
                                    if disabled & mask == mask {
                                        continue;
                                    }
                                    let (_, fut, ..) = &mut futures;
                                    let mut fut = unsafe { Pin::new_unchecked(fut) };
                                    let out = match fut.poll(cx) {
                                        Ready(out) => out,
                                        Pending => {
                                            is_pending = true;
                                            continue;
                                        }
                                    };
                                    disabled |= mask;
                                    #[allow(unused_variables)]
                                    #[allow(unused_mut)]
                                    match &out {
                                        result => {}
                                        _ => continue,
                                    }
                                    return Ready(util::Out::_1(out));
                                }
                                _ => ::core::panicking::panic_fmt(::core::fmt::Arguments::new_v1(
                                    &["internal error: entered unreachable code: "],
                                    &match (
                                        &"reaching this means there probably is an off by one bug",
                                    ) {
                                        (arg0,) => [::core::fmt::ArgumentV1::new(
                                            arg0,
                                            ::core::fmt::Display::fmt,
                                        )],
                                    },
                                )),
                            }
                        }
                        if is_pending {
                            Pending
                        } else {
                            Ready(util::Out::Disabled)
                        }
                    })
                    .await
                };
                match output {
                    util::Out::_0(()) => (),
                    util::Out::_1(result) => {
                        let _ = __reply_tx.send(result);
                    }
                    util::Out::Disabled => ::std::rt::begin_panic(
                        "all branches are disabled and there is no else branch",
                    ),
                    _ => ::core::panicking::panic_fmt(::core::fmt::Arguments::new_v1(
                        &["internal error: entered unreachable code: "],
                        &match (&"failed to match bind",) {
                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                arg0,
                                ::core::fmt::Display::fmt,
                            )],
                        },
                    )),
                }
            }
            Self::__Phantom(_) => (),
        }
    }
}
///Remote server for [#ident] taking the target object by value.
pub struct MyGenericServiceServer<T, Target, Codec> {
    target: Target,
    req_rx: ::remoc::rch::mpsc::Receiver<
        ::remoc::rtc::Req<
            MyGenericServiceReqValue<T, Codec>,
            MyGenericServiceReqRef<T, Codec>,
            MyGenericServiceReqRefMut<T, Codec>,
        >,
        Codec,
    >,
}
impl<T, Target, Codec> ::remoc::rtc::ServerBase for MyGenericServiceServer<T, Target, Codec>
where
    T: ::remoc::RemoteSend,
    Codec: ::remoc::codec::Codec,
    Target: MyGenericService<T, Codec>,
{
    type Client = MyGenericServiceClient<T, Codec>;
}
impl<T, Target, Codec> ::remoc::rtc::Server<Target, Codec>
    for MyGenericServiceServer<T, Target, Codec>
where
    T: ::remoc::RemoteSend,
    Codec: ::remoc::codec::Codec,
    Target: MyGenericService<T, Codec>,
{
    fn new(target: Target, request_buffer: usize) -> (Self, Self::Client) {
        let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
        (Self { target, req_rx }, Self::Client { req_tx })
    }
    #[allow(
        clippy::let_unit_value,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds,
        clippy::used_underscore_binding
    )]
    fn serve<'async_trait>(
        self,
    ) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = Option<Target>> + 'async_trait>>
    where
        Self: 'async_trait,
    {
        Box::pin(async move {
            if let ::core::option::Option::Some(__ret) =
                ::core::option::Option::None::<Option<Target>>
            {
                return __ret;
            }
            let __self = self;
            let __ret: Option<Target> = {
                let Self {
                    mut target,
                    mut req_rx,
                } = __self;
                loop {
                    match req_rx.recv().await {
                        Ok(Some(::remoc::rtc::Req::Value(req))) => {
                            req.dispatch(target).await;
                            return None;
                        }
                        Ok(Some(::remoc::rtc::Req::Ref(req))) => {
                            req.dispatch(&target).await;
                        }
                        Ok(Some(::remoc::rtc::Req::RefMut(req))) => {
                            req.dispatch(&mut target).await;
                        }
                        Ok(None) => return Some(target),
                        Err(err) => ::remoc::rtc::receiving_request_failed(err),
                    }
                }
            };
            #[allow(unreachable_code)]
            __ret
        })
    }
}
///Remote server for [#ident] taking the target object by mutable reference.
pub struct MyGenericServiceServerRefMut<'target, T, Target, Codec> {
    target: &'target mut Target,
    req_rx: ::remoc::rch::mpsc::Receiver<
        ::remoc::rtc::Req<
            MyGenericServiceReqValue<T, Codec>,
            MyGenericServiceReqRef<T, Codec>,
            MyGenericServiceReqRefMut<T, Codec>,
        >,
        Codec,
    >,
}
impl<'target, T, Target, Codec> ::remoc::rtc::ServerBase
    for MyGenericServiceServerRefMut<'target, T, Target, Codec>
where
    T: ::remoc::RemoteSend,
    Codec: ::remoc::codec::Codec,
    Target: MyGenericService<T, Codec>,
{
    type Client = MyGenericServiceClient<T, Codec>;
}
impl<'target, T, Target, Codec> ::remoc::rtc::ServerRefMut<'target, Target, Codec>
    for MyGenericServiceServerRefMut<'target, T, Target, Codec>
where
    T: ::remoc::RemoteSend,
    Codec: ::remoc::codec::Codec,
    Target: MyGenericService<T, Codec>,
{
    fn new(target: &'target mut Target, request_buffer: usize) -> (Self, Self::Client) {
        let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
        (Self { target, req_rx }, Self::Client { req_tx })
    }
    #[allow(
        clippy::let_unit_value,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds,
        clippy::used_underscore_binding
    )]
    fn serve<'async_trait>(
        self,
    ) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = ()> + 'async_trait>>
    where
        Self: 'async_trait,
    {
        Box::pin(async move {
            let __self = self;
            let _: () = {
                let Self { target, mut req_rx } = __self;
                loop {
                    match req_rx.recv().await {
                        Ok(Some(::remoc::rtc::Req::Ref(req))) => {
                            req.dispatch(target).await;
                        }
                        Ok(Some(::remoc::rtc::Req::RefMut(req))) => {
                            req.dispatch(target).await;
                        }
                        Ok(Some(_)) => (),
                        Ok(None) => break,
                        Err(err) => ::remoc::rtc::receiving_request_failed(err),
                    }
                }
            };
        })
    }
}
///Remote server for [#ident] taking the target object by shared mutable reference.
pub struct MyGenericServiceServerSharedMut<T, Target, Codec> {
    target: ::std::sync::Arc<::remoc::rtc::LocalRwLock<Target>>,
    req_rx: ::remoc::rch::mpsc::Receiver<
        ::remoc::rtc::Req<
            MyGenericServiceReqValue<T, Codec>,
            MyGenericServiceReqRef<T, Codec>,
            MyGenericServiceReqRefMut<T, Codec>,
        >,
        Codec,
    >,
}
impl<T, Target, Codec> ::remoc::rtc::ServerBase
    for MyGenericServiceServerSharedMut<T, Target, Codec>
where
    T: ::remoc::RemoteSend,
    Codec: ::remoc::codec::Codec,
    Target: MyGenericService<T, Codec>,
    Target: ::std::marker::Send + ::std::marker::Sync + 'static,
{
    type Client = MyGenericServiceClient<T, Codec>;
}
impl<T, Target, Codec> ::remoc::rtc::ServerSharedMut<Target, Codec>
    for MyGenericServiceServerSharedMut<T, Target, Codec>
where
    T: ::remoc::RemoteSend,
    Codec: ::remoc::codec::Codec,
    Target: MyGenericService<T, Codec>,
    Target: ::std::marker::Send + ::std::marker::Sync + 'static,
{
    fn new(
        target: ::std::sync::Arc<::remoc::rtc::LocalRwLock<Target>>,
        request_buffer: usize,
    ) -> (Self, Self::Client) {
        let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
        (Self { target, req_rx }, Self::Client { req_tx })
    }
    #[allow(
        clippy::let_unit_value,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds,
        clippy::used_underscore_binding
    )]
    fn serve<'async_trait>(
        self,
        spawn: bool,
    ) -> ::core::pin::Pin<
        Box<dyn ::core::future::Future<Output = ()> + ::core::marker::Send + 'async_trait>,
    >
    where
        Self: 'async_trait,
    {
        Box::pin(async move {
            let __self = self;
            let spawn = spawn;
            let _: () = {
                let Self { target, mut req_rx } = __self;
                loop {
                    match req_rx.recv().await {
                        Ok(Some(::remoc::rtc::Req::Ref(req))) => {
                            if spawn {
                                let target = target.clone().read_owned().await;
                                ::remoc::rtc::spawn(async move {
                                    req.dispatch(&*target).await;
                                });
                            } else {
                                let target = target.read().await;
                                req.dispatch(&*target).await;
                            }
                        }
                        Ok(Some(::remoc::rtc::Req::RefMut(req))) => {
                            let mut target = target.write().await;
                            req.dispatch(&mut *target).await;
                        }
                        Ok(Some(_)) => (),
                        Ok(None) => break,
                        Err(err) => ::remoc::rtc::receiving_request_failed(err),
                    }
                }
            };
        })
    }
}
#[serde(bound(serialize = "Codec: ::remoc::codec::Codec"))]
#[serde(bound(deserialize = "Codec: ::remoc::codec::Codec"))]
pub struct MyGenericServiceClient<T, Codec> {
    req_tx: ::remoc::rch::mpsc::Sender<
        ::remoc::rtc::Req<
            MyGenericServiceReqValue<T, Codec>,
            MyGenericServiceReqRef<T, Codec>,
            MyGenericServiceReqRefMut<T, Codec>,
        >,
        Codec,
    >,
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<T, Codec> _serde::Serialize for MyGenericServiceClient<T, Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn serialize<__S>(
            &self,
            __serializer: __S,
        ) -> _serde::__private::Result<__S::Ok, __S::Error>
        where
            __S: _serde::Serializer,
        {
            let mut __serde_state = match _serde::Serializer::serialize_struct(
                __serializer,
                "MyGenericServiceClient",
                false as usize + 1,
            ) {
                _serde::__private::Ok(__val) => __val,
                _serde::__private::Err(__err) => {
                    return _serde::__private::Err(__err);
                }
            };
            match _serde::ser::SerializeStruct::serialize_field(
                &mut __serde_state,
                "req_tx",
                &self.req_tx,
            ) {
                _serde::__private::Ok(__val) => __val,
                _serde::__private::Err(__err) => {
                    return _serde::__private::Err(__err);
                }
            };
            _serde::ser::SerializeStruct::end(__serde_state)
        }
    }
};
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<'de, T, Codec> _serde::Deserialize<'de> for MyGenericServiceClient<T, Codec>
    where
        Codec: ::remoc::codec::Codec,
    {
        fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
        where
            __D: _serde::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            enum __Field {
                __field0,
                __ignore,
            }
            struct __FieldVisitor;
            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                type Value = __Field;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "field identifier")
                }
                fn visit_u64<__E>(self, __value: u64) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        0u64 => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
                fn visit_str<__E>(
                    self,
                    __value: &str,
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "req_tx" => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
                fn visit_bytes<__E>(
                    self,
                    __value: &[u8],
                ) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"req_tx" => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(
                    __deserializer: __D,
                ) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            struct __Visitor<'de, T, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                marker: _serde::__private::PhantomData<MyGenericServiceClient<T, Codec>>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de, T, Codec> _serde::de::Visitor<'de> for __Visitor<'de, T, Codec>
            where
                Codec: ::remoc::codec::Codec,
            {
                type Value = MyGenericServiceClient<T, Codec>;
                fn expecting(
                    &self,
                    __formatter: &mut _serde::__private::Formatter,
                ) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(
                        __formatter,
                        "struct MyGenericServiceClient",
                    )
                }
                #[inline]
                fn visit_seq<__A>(
                    self,
                    mut __seq: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::SeqAccess<'de>,
                {
                    let __field0 = match match _serde::de::SeqAccess::next_element::<
                        ::remoc::rch::mpsc::Sender<
                            ::remoc::rtc::Req<
                                MyGenericServiceReqValue<T, Codec>,
                                MyGenericServiceReqRef<T, Codec>,
                                MyGenericServiceReqRefMut<T, Codec>,
                            >,
                            Codec,
                        >,
                    >(&mut __seq)
                    {
                        _serde::__private::Ok(__val) => __val,
                        _serde::__private::Err(__err) => {
                            return _serde::__private::Err(__err);
                        }
                    } {
                        _serde::__private::Some(__value) => __value,
                        _serde::__private::None => {
                            return _serde::__private::Err(_serde::de::Error::invalid_length(
                                0usize,
                                &"struct MyGenericServiceClient with 1 element",
                            ));
                        }
                    };
                    _serde::__private::Ok(MyGenericServiceClient { req_tx: __field0 })
                }
                #[inline]
                fn visit_map<__A>(
                    self,
                    mut __map: __A,
                ) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::MapAccess<'de>,
                {
                    let mut __field0: _serde::__private::Option<
                        ::remoc::rch::mpsc::Sender<
                            ::remoc::rtc::Req<
                                MyGenericServiceReqValue<T, Codec>,
                                MyGenericServiceReqRef<T, Codec>,
                                MyGenericServiceReqRefMut<T, Codec>,
                            >,
                            Codec,
                        >,
                    > = _serde::__private::None;
                    while let _serde::__private::Some(__key) =
                        match _serde::de::MapAccess::next_key::<__Field>(&mut __map) {
                            _serde::__private::Ok(__val) => __val,
                            _serde::__private::Err(__err) => {
                                return _serde::__private::Err(__err);
                            }
                        }
                    {
                        match __key {
                            __Field::__field0 => {
                                if _serde::__private::Option::is_some(&__field0) {
                                    return _serde::__private::Err(
                                        <__A::Error as _serde::de::Error>::duplicate_field(
                                            "req_tx",
                                        ),
                                    );
                                }
                                __field0 = _serde::__private::Some(
                                    match _serde::de::MapAccess::next_value::<
                                        ::remoc::rch::mpsc::Sender<
                                            ::remoc::rtc::Req<
                                                MyGenericServiceReqValue<T, Codec>,
                                                MyGenericServiceReqRef<T, Codec>,
                                                MyGenericServiceReqRefMut<T, Codec>,
                                            >,
                                            Codec,
                                        >,
                                    >(&mut __map)
                                    {
                                        _serde::__private::Ok(__val) => __val,
                                        _serde::__private::Err(__err) => {
                                            return _serde::__private::Err(__err);
                                        }
                                    },
                                );
                            }
                            _ => {
                                let _ = match _serde::de::MapAccess::next_value::<
                                    _serde::de::IgnoredAny,
                                >(&mut __map)
                                {
                                    _serde::__private::Ok(__val) => __val,
                                    _serde::__private::Err(__err) => {
                                        return _serde::__private::Err(__err);
                                    }
                                };
                            }
                        }
                    }
                    let __field0 = match __field0 {
                        _serde::__private::Some(__field0) => __field0,
                        _serde::__private::None => {
                            match _serde::__private::de::missing_field("req_tx") {
                                _serde::__private::Ok(__val) => __val,
                                _serde::__private::Err(__err) => {
                                    return _serde::__private::Err(__err);
                                }
                            }
                        }
                    };
                    _serde::__private::Ok(MyGenericServiceClient { req_tx: __field0 })
                }
            }
            const FIELDS: &'static [&'static str] = &["req_tx"];
            _serde::Deserializer::deserialize_struct(
                __deserializer,
                "MyGenericServiceClient",
                FIELDS,
                __Visitor {
                    marker: _serde::__private::PhantomData::<MyGenericServiceClient<T, Codec>>,
                    lifetime: _serde::__private::PhantomData,
                },
            )
        }
    }
};
impl<T, Codec> MyGenericService<T, Codec> for MyGenericServiceClient<T, Codec>
where
    T: ::remoc::RemoteSend,
    Codec: ::remoc::codec::Codec,
{
    #[allow(
        clippy::let_unit_value,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds,
        clippy::used_underscore_binding
    )]
    fn mut_fn<'life0, 'async_trait>(
        &'life0 mut self,
        arg1: T,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<(), MyError>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            if let ::core::option::Option::Some(__ret) =
                ::core::option::Option::None::<Result<(), MyError>>
            {
                return __ret;
            }
            let mut __self = self;
            let arg1 = arg1;
            let __ret: Result<(), MyError> = {
                let (reply_tx, reply_rx) = ::remoc::rch::oneshot::channel();
                let req_value = MyGenericServiceReqRefMut::MutFn {
                    __reply_tx: reply_tx,
                    arg1,
                };
                let req = ::remoc::rtc::Req::RefMut(req_value);
                __self
                    .req_tx
                    .send(req)
                    .await
                    .map_err(::remoc::rtc::CallError::from)?;
                let reply = reply_rx.await.map_err(::remoc::rtc::CallError::from)?;
                reply
            };
            #[allow(unreachable_code)]
            __ret
        })
    }
}
impl<T, Codec> ::std::fmt::Debug for MyGenericServiceClient<T, Codec>
where
    T: ::remoc::RemoteSend,
{
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.write_fmt(::core::fmt::Arguments::new_v1(
            &["#client_ident"],
            &match () {
                () => [],
            },
        ))
    }
}
impl<T, Codec> ::std::ops::Drop for MyGenericServiceClient<T, Codec>
where
    T: ::remoc::RemoteSend,
{
    fn drop(&mut self) {}
}
