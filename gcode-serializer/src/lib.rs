use serde::{
    ser::{self, SerializeStruct},
    Serialize,
};

use std::sync::{atomic::AtomicI32 as Ai32, atomic::Ordering, Arc};

pub const SEQUENCE_START: i32 = 1;

#[derive(Debug, Clone)]
pub struct Sequenced {
    sequence: Arc<Ai32>,
}

impl Default for Sequenced {
    fn default() -> Self {
        Self {
            sequence: Arc::new(SEQUENCE_START.into()),
        }
    }
}

pub fn serialize_unsequenced(t: impl Serialize) -> Box<[u8]> {
    let mut line = GcodeLine::new();
    line.serialize(t);
    line.finish()
}

impl Sequenced {
    /// Format the given serializable into the internal buffer, then split
    /// off the bytes and return a handle to them.
    ///
    /// Sequence number (N<seq>) and checksum (*<sum>) are automatically handled,
    /// the sequence number of the line is returned with the output for external tracking.
    pub fn serialize(&self, t: impl Serialize) -> (i32, Box<[u8]>) {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);
        let mut line = GcodeLine::new();
        line.serialize('N').serialize(sequence).serialize(t);
        let bytes = line.finish_with_checksum();
        (sequence, bytes)
    }

    /// Format the given serializable into the internal buffer, then split
    /// off the bytes and return the handle to them.
    ///
    /// No sequnce number or checksum are added, internal state does not change.
    pub fn serialize_unsequenced(&self, t: impl Serialize) -> Box<[u8]> {
        serialize_unsequenced(t)
    }

    /// Crate a new serializer
    pub fn new() -> Self {
        Default::default()
    }

    /// Sets the internal sequence counter to the provided integer.
    /// This also affects all serializers cloned from this instance.
    ///
    /// Serializer instances load the sequence counter very early in serialization,
    /// thus if another thread is serializing when the sequence is set, it will not
    /// apply to anything that has already begun to be serialized.
    ///
    /// Note: Sometimes devices need to be told when sequence numbers don't change sequentially;
    /// for instance Marlin 3D printers require an `M110 N<seq>` to change line number.
    pub fn set_sequence(&self, new_sequence: i32) {
        self.sequence.store(new_sequence, Ordering::SeqCst);
    }
}

#[derive(Debug, Default)]
struct GcodeLine {
    buffer: Vec<u8>,
    checksum: u8,
}

impl GcodeLine {
    fn new() -> Self {
        Self {
            buffer: Vec::new(),
            checksum: 0,
        }
    }
    fn checksum(&mut self, buf: &[u8]) {
        for byte in buf {
            self.checksum ^= byte;
        }
    }
    fn write(&mut self, buf: &[u8]) {
        self.buffer.extend_from_slice(buf);
        self.checksum(buf);
    }
    fn serialize(&mut self, t: impl Serialize) -> &mut Self {
        t.serialize(&mut *self).expect("Infallible");
        self
    }

    fn finish_with_checksum(mut self) -> Box<[u8]> {
        self.buffer.push(b'*');
        self.buffer
            .extend_from_slice(itoa::Buffer::new().format(self.checksum).as_bytes());
        self.finish()
    }

    /// finish the current line and give the sequence number of it for tracking, 0 for unsequenced
    fn finish(mut self) -> Box<[u8]> {
        self.buffer.push(b'\n');
        self.buffer.into_boxed_slice()
    }
}

impl ser::Serializer for &mut GcodeLine {
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

impl ser::SerializeSeq for &mut GcodeLine {
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

impl ser::SerializeMap for &mut GcodeLine {
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

impl ser::SerializeStruct for &mut GcodeLine {
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

impl ser::SerializeStructVariant for &mut GcodeLine {
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

impl ser::SerializeTuple for &mut GcodeLine {
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

impl ser::SerializeTupleStruct for &mut GcodeLine {
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

impl ser::SerializeTupleVariant for &mut GcodeLine {
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

    #[test]
    fn unit_serialize_works() {
        let writer = Sequenced::default();
        let out = writer.serialize_unsequenced(M1234);
        let expected: &[u8] = b"M1234\n";
        assert_eq!(out.as_ref(), expected);

        let out = writer.serialize(G1234 { x: -1, y: 2.3 });
        let expected: &[u8] = b"N1G1234X-1Y2.3*14\n";
        assert_eq!(out.1.as_ref(), expected);
    }

    #[test]
    fn atomic_counter() {
        let writer1 = Sequenced::default();
        let writer2 = writer1.clone();

        let out = writer1.serialize(G1234 { x: -1, y: 2.3 });
        let expected: &[u8] = b"N1G1234X-1Y2.3*14\n";
        assert_eq!(out.1.as_ref(), expected);

        std::thread::spawn(move || {
            let out = writer2.serialize(G1234 { x: -1, y: 2.3 });
            let expected: &[u8] = b"N2G1234X-1Y2.3*13\n";
            assert_eq!(out.1.as_ref(), expected);
        })
        .join()
        .unwrap();

        let out = writer1.serialize(G1234 { x: -1, y: 2.3 });
        let expected: &[u8] = b"N3G1234X-1Y2.3*12\n";
        assert_eq!(out.1.as_ref(), expected);
    }
}
