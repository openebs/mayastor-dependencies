use std::io::{IoSliceMut, Read};

/// This is io::Read capable Sized type. This can wrap around a Vec<u8>.
pub struct ByteBuf {
    inner: Vec<u8>,
}

impl Read for ByteBuf {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.as_slice().read(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> std::io::Result<usize> {
        self.inner.as_slice().read_vectored(bufs)
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        self.inner.as_slice().read_to_end(buf)
    }

    fn read_to_string(&mut self, buf: &mut String) -> std::io::Result<usize> {
        self.inner.as_slice().read_to_string(buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        self.inner.as_slice().read_exact(buf)
    }

    fn by_ref(&mut self) -> &mut Self
    where
        Self: Sized,
    {
        self
    }
}

impl From<Vec<u8>> for ByteBuf {
    fn from(inner: Vec<u8>) -> Self {
        Self { inner }
    }
}
