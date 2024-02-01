use serde::{
    ser::{self, SerializeStruct},
    Serialize,
};

use core::sync::atomic::{AtomicI32 as Ai32, Ordering};

use bytes::{BufMut, Bytes, BytesMut};

static SEQUENCE: Ai32 = Ai32::new(1);

#[derive(Debug)]
pub struct Serializer<B = BytesMut> {
    buffer: B,
}

pub type UnbufferedSerializer = Serializer<()>;

impl Default for Serializer {
    fn default() -> Self {
        Self {
            buffer: BytesMut::with_capacity(128),
        }
    }
}

impl Default for UnbufferedSerializer {
    fn default() -> Self {
        Self {
            buffer: Default::default(),
        }
    }
}

impl Serializer {
    fn start_line(&mut self) -> GcodeLineWriter<BytesMut> {
        // seqcst likely overkill, needs testing to relax
        let sequence = SEQUENCE.fetch_add(1, Ordering::SeqCst);
        let mut line = GcodeLineWriter {
            buffer: &mut self.buffer,
            sequence: Some(sequence),
            checksum: 0,
        };
        line.serialize('N').serialize(sequence);
        line
    }
    pub fn serialize(&mut self, t: impl Serialize) -> Bytes {
        self.start_line().serialize(t).finish();
        self.buffer.split().freeze()
    }

    pub fn serialize_unsequenced(&self, t: impl Serialize) -> Bytes {
        let mut temp_buffer = BytesMut::new();
        self.serialize_unsequenced_into(&mut temp_buffer, t);
        temp_buffer.split().freeze()
    }
}

impl<B> Serializer<B> {
    pub fn new(buffer: B) -> Self {
        Self { buffer }
    }

    pub fn serialize_into(&mut self, buffer: &mut impl BufMut, t: impl Serialize) {
        let sequence = SEQUENCE.fetch_add(1, Ordering::SeqCst);
        let mut line_writer = GcodeLineWriter {
            buffer,
            sequence: Some(sequence),
            checksum: 0,
        };
        line_writer
            .serialize('N')
            .serialize(sequence)
            .serialize(t)
            .finish();
    }

    pub fn serialize_unsequenced_into(&self, buffer: &mut impl BufMut, t: impl Serialize) {
        let mut line_writer = GcodeLineWriter {
            buffer,
            sequence: None,
            checksum: 0,
        };
        line_writer.serialize(t).finish();
    }
}

#[derive(Debug)]
struct GcodeLineWriter<'a, B> {
    buffer: &'a mut B,
    sequence: Option<i32>,
    checksum: u8,
}

impl<'a, B> GcodeLineWriter<'a, B>
where
    B: BufMut,
{
    fn checksum(&mut self, buf: &[u8]) {
        for byte in buf {
            self.checksum ^= byte;
        }
    }
    fn write(&mut self, buf: &[u8]) {
        self.buffer.put_slice(buf);
        self.checksum(buf);
    }
    fn serialize(&mut self, t: impl Serialize) -> &mut Self {
        t.serialize(&mut *self).expect("Infallible");
        self
    }
    fn finish(&mut self) {
        if let Some(_sequence) = self.sequence {
            self.buffer.put_u8(b'*');
            self.buffer
                .put(itoa::Buffer::new().format(self.checksum).as_bytes());
        };
        self.buffer.put_u8(b'\n');
    }
}

impl<'item, 'line, B> ser::Serializer for &'item mut GcodeLineWriter<'line, B>
where
    'line: 'item,
    B: BufMut,
{
    type Ok = ();

    type Error = core::fmt::Error;

    type SerializeSeq = Self;

    type SerializeTuple = Self;

    type SerializeTupleStruct = Self;

    type SerializeTupleVariant = Self;

    type SerializeMap = Self;

    type SerializeStruct = Self;

    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        let mut buf = itoa::Buffer::new();
        let buf = buf.format(v as u8).as_bytes();
        self.write(buf);
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        let mut buf = itoa::Buffer::new();
        let buf = buf.format(v).as_bytes();
        self.write(buf);
        Ok(())
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        let mut buf = itoa::Buffer::new();
        let buf = buf.format(v).as_bytes();
        self.write(buf);
        Ok(())
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        let mut buf = itoa::Buffer::new();
        let buf = buf.format(v).as_bytes();
        self.write(buf);
        Ok(())
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        let mut buf = itoa::Buffer::new();
        let buf = buf.format(v).as_bytes();
        self.write(buf);
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        let mut buf = itoa::Buffer::new();
        let buf = buf.format(v).as_bytes();
        self.write(buf);
        Ok(())
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        let mut buf = itoa::Buffer::new();
        let buf = buf.format(v).as_bytes();
        self.write(buf);
        Ok(())
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        let mut buf = itoa::Buffer::new();
        let buf = buf.format(v).as_bytes();
        self.write(buf);
        Ok(())
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        let mut buf = itoa::Buffer::new();
        let buf = buf.format(v).as_bytes();
        self.write(buf);
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        let mut buf = ryu::Buffer::new();
        let buf = buf.format(v).as_bytes();
        self.write(buf);
        Ok(())
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        let mut buf = ryu::Buffer::new();
        let buf = buf.format(v).as_bytes();
        self.write(buf);
        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        let mut buffer = [0; 4];
        let buf = v.encode_utf8(&mut buffer).as_bytes();
        self.write(buf);
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        let buf = v.as_bytes();
        self.write(buf);
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        self.write(v);
        Ok(())
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_some<T: ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        name.serialize(self)
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.serialize_unit_struct(variant)
    }

    fn serialize_newtype_struct<T: ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize,
    {
        value.serialize(self)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(self)
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(self)
    }

    fn serialize_struct(
        self,
        name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        name.serialize(&mut *self)?;
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        self.serialize_struct(variant, len)
    }
}

impl<'item, 'line, B> ser::SerializeSeq for &'item mut GcodeLineWriter<'line, B>
where
    'line: 'item,
    B: BufMut,
{
    type Ok = ();

    type Error = core::fmt::Error;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'item, 'line, B> ser::SerializeMap for &'item mut GcodeLineWriter<'line, B>
where
    'line: 'item,
    B: BufMut,
{
    type Ok = ();

    type Error = core::fmt::Error;

    fn serialize_key<T: ?Sized>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        key.serialize(&mut **self)
    }

    fn serialize_value<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'item, 'line, B> ser::SerializeStruct for &'item mut GcodeLineWriter<'line, B>
where
    'line: 'item,
    B: BufMut,
{
    type Ok = ();

    type Error = core::fmt::Error;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        key.chars()
            .nth(0)
            .unwrap()
            .to_ascii_uppercase()
            .serialize(&mut **self)
            .expect("Infallible");
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'item, 'line, B> ser::SerializeStructVariant for &'item mut GcodeLineWriter<'line, B>
where
    'line: 'item,
    B: BufMut,
{
    type Ok = ();

    type Error = core::fmt::Error;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        <Self as SerializeStruct>::serialize_field(self, key, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        <Self as SerializeStruct>::end(self)
    }
}

impl<'item, 'line, B> ser::SerializeTuple for &'item mut GcodeLineWriter<'line, B>
where
    'line: 'item,
    B: BufMut,
{
    type Ok = ();

    type Error = core::fmt::Error;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'item, 'line, B> ser::SerializeTupleStruct for &'item mut GcodeLineWriter<'line, B>
where
    'line: 'item,
    B: BufMut,
{
    type Ok = ();

    type Error = core::fmt::Error;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'item, 'line, B> ser::SerializeTupleVariant for &'item mut GcodeLineWriter<'line, B>
where
    'line: 'item,
    B: BufMut,
{
    type Ok = ();

    type Error = core::fmt::Error;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct M1234;

    #[derive(Serialize)]
    struct G1234 {
        x: i32,
        y: f32,
    }

    static SEQUENCE_LOCK: std::sync::OnceLock<std::sync::Arc<std::sync::Mutex<()>>> =
        std::sync::OnceLock::new();

    fn locker() -> std::sync::Arc<std::sync::Mutex<()>> {
        let a_sequence =
            SEQUENCE_LOCK.get_or_init(|| std::sync::Arc::new(std::sync::Mutex::default()));
        a_sequence.clone()
    }

    #[test]
    fn unit_serialize_works() {
        let locker = locker();
        let _lock = locker.lock();
        SEQUENCE.store(1, Ordering::SeqCst);
        let mut writer = Serializer::default();
        let out = writer.serialize_unsequenced(M1234);
        let expected: &[u8] = b"M1234\n";
        assert_eq!(out, expected);

        let out = writer.serialize(G1234 { x: -1, y: 2.3 });
        let expected: &[u8] = b"N1G1234X-1Y2.3*14\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn atomic_counter() {
        let locker = locker();
        let _lock = locker.lock();
        SEQUENCE.store(1, Ordering::SeqCst);
        let mut writer1 = Serializer::default();
        let mut writer2 = Serializer::default();

        let out = writer1.serialize(G1234 { x: -1, y: 2.3 });
        let expected: &[u8] = b"N1G1234X-1Y2.3*14\n";
        assert_eq!(out, expected);

        std::thread::spawn(move || {
            let out = writer2.serialize(G1234 { x: -1, y: 2.3 });
            let expected: &[u8] = b"N2G1234X-1Y2.3*13\n";
            assert_eq!(out, expected);
        })
        .join()
        .unwrap();

        let out = writer1.serialize(G1234 { x: -1, y: 2.3 });
        let expected: &[u8] = b"N3G1234X-1Y2.3*12\n";
        assert_eq!(out, expected);
    }
}
