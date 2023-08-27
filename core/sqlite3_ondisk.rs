/// SQLite on-disk file format.
///
/// SQLite stores data in a single database file, which is divided into fixed-size
/// pages:
///
/// ```text
/// +----------+----------+----------+-----------------------------+----------+
/// |          |          |          |                             |          |
/// |  Page 1  |  Page 2  |  Page 3  |           ...               |  Page N  |
/// |          |          |          |                             |          |
/// +----------+----------+----------+-----------------------------+----------+
/// ```
///
/// The first page is special because it contains a 100 byte header at the beginning.
///
/// Each page constists of a page header and N cells, which contain the records.
///
/// +-----------------+----------------+---------------------+----------------+
/// |                 |                |                     |                |
/// |   Page header   |  Cell pointer  |     Unallocated     |  Cell content  |
/// | (8 or 12 bytes) |     array      |        space        |      area      |      
/// |                 |                |                     |                |
/// +-----------------+----------------+---------------------+----------------+
///
/// For more information, see: https://www.sqlite.org/fileformat.html
use crate::buffer_pool::BufferPool;
use crate::{DatabaseRef, IO};
use anyhow::{anyhow, Result};
use std::borrow::BorrowMut;
use std::sync::Arc;

/// The size of the database header in bytes.
pub const DATABASE_HEADER_SIZE: usize = 100;

#[derive(Debug, Default)]
pub struct DatabaseHeader {
    magic: [u8; 16],
    pub page_size: u16,
    write_version: u8,
    read_version: u8,
    unused_space: u8,
    max_embed_frac: u8,
    min_embed_frac: u8,
    min_leaf_frac: u8,
    change_counter: u32,
    database_size: u32,
    freelist_trunk_page: u32,
    freelist_pages: u32,
    schema_cookie: u32,
    schema_format: u32,
    default_cache_size: u32,
    vacuum: u32,
    text_encoding: u32,
    user_version: u32,
    incremental_vacuum: u32,
    application_id: u32,
    reserved: [u8; 20],
    version_valid_for: u32,
    version_number: u32,
}

pub fn read_database_header(io: Arc<dyn IO>, database_ref: DatabaseRef) -> Result<DatabaseHeader> {
    let mut buf = [0; 512];
    io.get(database_ref, 1, &mut buf)?;
    let mut header = DatabaseHeader::default();
    header.magic.copy_from_slice(&buf[0..16]);
    header.page_size = u16::from_be_bytes([buf[16], buf[17]]);
    header.write_version = buf[18];
    header.read_version = buf[19];
    header.unused_space = buf[20];
    header.max_embed_frac = buf[21];
    header.min_embed_frac = buf[22];
    header.min_leaf_frac = buf[23];
    header.change_counter = u32::from_be_bytes([buf[24], buf[25], buf[26], buf[27]]);
    header.database_size = u32::from_be_bytes([buf[28], buf[29], buf[30], buf[31]]);
    header.freelist_trunk_page = u32::from_be_bytes([buf[32], buf[33], buf[34], buf[35]]);
    header.freelist_pages = u32::from_be_bytes([buf[36], buf[37], buf[38], buf[39]]);
    header.schema_cookie = u32::from_be_bytes([buf[40], buf[41], buf[42], buf[43]]);
    header.schema_format = u32::from_be_bytes([buf[44], buf[45], buf[46], buf[47]]);
    header.default_cache_size = u32::from_be_bytes([buf[48], buf[49], buf[50], buf[51]]);
    header.vacuum = u32::from_be_bytes([buf[52], buf[53], buf[54], buf[55]]);
    header.text_encoding = u32::from_be_bytes([buf[56], buf[57], buf[58], buf[59]]);
    header.user_version = u32::from_be_bytes([buf[60], buf[61], buf[62], buf[63]]);
    header.incremental_vacuum = u32::from_be_bytes([buf[64], buf[65], buf[66], buf[67]]);
    header.application_id = u32::from_be_bytes([buf[68], buf[69], buf[70], buf[71]]);
    header.reserved.copy_from_slice(&buf[72..92]);
    header.version_valid_for = u32::from_be_bytes([buf[92], buf[93], buf[94], buf[95]]);
    header.version_number = u32::from_be_bytes([buf[96], buf[97], buf[98], buf[99]]);
    Ok(header)
}

#[derive(Debug)]
pub struct BTreePageHeader {
    page_type: PageType,
    _first_freeblock_offset: u16,
    num_cells: u16,
    _cell_content_area: u16,
    _num_frag_free_bytes: u8,
    right_most_pointer: Option<u32>,
}

#[repr(u8)]
#[derive(Debug, PartialEq)]
pub enum PageType {
    IndexInterior = 2,
    TableInterior = 5,
    IndexLeaf = 10,
    TableLeaf = 13,
}

impl TryFrom<u8> for PageType {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            2 => Ok(Self::IndexInterior),
            5 => Ok(Self::TableInterior),
            10 => Ok(Self::IndexLeaf),
            13 => Ok(Self::TableLeaf),
            _ => Err(anyhow!("Invalid page type: {}", value)),
        }
    }
}

#[derive(Debug)]
pub struct BTreePage {
    pub header: BTreePageHeader,
    pub cells: Vec<BTreeCell>,
}

pub fn read_btree_page(
    io: Arc<dyn IO>,
    database_ref: DatabaseRef,
    buffer_pool: &mut BufferPool,
    page_idx: usize,
) -> Result<BTreePage> {
    let mut buf = buffer_pool.get();
    let page = &mut buf.borrow_mut().data_mut();
    io.get(database_ref, page_idx, page)?;
    let mut pos = if page_idx == 1 {
        DATABASE_HEADER_SIZE
    } else {
        0
    };
    let mut header = BTreePageHeader {
        page_type: page[pos].try_into()?,
        _first_freeblock_offset: u16::from_be_bytes([page[pos + 1], page[pos + 2]]),
        num_cells: u16::from_be_bytes([page[pos + 3], page[pos + 4]]),
        _cell_content_area: u16::from_be_bytes([page[pos + 5], page[pos + 6]]),
        _num_frag_free_bytes: page[pos + 7],
        right_most_pointer: None,
    };
    pos += 8;
    if header.page_type == PageType::IndexInterior || header.page_type == PageType::TableInterior {
        header.right_most_pointer = Some(u32::from_be_bytes([
            page[pos],
            page[pos + 1],
            page[pos + 2],
            page[pos + 3],
        ]));
        pos += 4;
    }
    let mut cells = Vec::new();
    for _ in 0..header.num_cells {
        let cell_pointer = u16::from_be_bytes([page[pos], page[pos + 1]]);
        pos += 2;
        let cell = read_btree_cell(page, &header.page_type, cell_pointer as usize)?;
        match &cell {
            BTreeCell::TableLeafCell(TableLeafCell { _rowid, _payload }) => {
                let record = read_record(_payload)?;
                println!("record: {:?}", record);
            }
        }
        cells.push(cell);
    }
    Ok(BTreePage { header, cells })
}

#[derive(Debug)]
pub enum BTreeCell {
    TableLeafCell(TableLeafCell),
}

#[derive(Debug)]
pub struct TableLeafCell {
    _rowid: u64,
    _payload: Vec<u8>,
}

pub fn read_btree_cell(page: &[u8], page_type: &PageType, pos: usize) -> Result<BTreeCell> {
    match page_type {
        PageType::IndexInterior => todo!(),
        PageType::TableInterior => todo!(),
        PageType::IndexLeaf => todo!(),
        PageType::TableLeaf => {
            let mut pos = pos;
            let (payload_size, nr) = read_varint(&page[pos..])?;
            pos += nr;
            let (rowid, nr) = read_varint(&page[pos..])?;
            pos += nr;
            let payload = &page[pos..pos + payload_size as usize];
            // FIXME: page overflows if the payload is too large
            Ok(BTreeCell::TableLeafCell(TableLeafCell {
                _rowid: rowid,
                _payload: payload.to_vec(),
            }))
        }
    }
}

#[derive(Debug)]
pub enum Value {
    Null,
    Integer(i64),
    Float(f64),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Debug)]
pub struct Record {
    _values: Vec<Value>,
}

#[derive(Debug)]
pub enum SerialType {
    Null,
    UInt8,
    BEInt16,
    BEInt24,
    BEInt32,
    BEInt48,
    BEInt64,
    BEFloat64,
    ConstInt0,
    ConstInt1,
    Blob(usize),
    String(usize),
}

impl TryFrom<u64> for SerialType {
    type Error = anyhow::Error;

    fn try_from(value: u64) -> Result<Self> {
        match value {
            0 => Ok(Self::Null),
            1 => Ok(Self::UInt8),
            2 => Ok(Self::BEInt16),
            3 => Ok(Self::BEInt24),
            4 => Ok(Self::BEInt32),
            5 => Ok(Self::BEInt48),
            6 => Ok(Self::BEInt64),
            7 => Ok(Self::BEFloat64),
            8 => Ok(Self::ConstInt0),
            9 => Ok(Self::ConstInt1),
            n if value > 12 && value % 2 == 0 => Ok(Self::Blob(((n - 12) / 2) as usize)),
            n if value > 13 && value % 2 == 1 => Ok(Self::String(((n - 13) / 2) as usize)),
            _ => Err(anyhow!("Invalid serial type: {}", value)),
        }
    }
}

pub fn read_record(payload: &[u8]) -> Result<Record> {
    let mut pos = 0;
    let (header_size, nr) = read_varint(payload)?;
    assert!((header_size as usize) >= nr);
    let mut header_size = (header_size as usize) - nr;
    pos += nr;
    let mut serial_types = Vec::new();
    while header_size > 0 {
        let (serial_type, nr) = read_varint(&payload[pos..])?;
        let serial_type = SerialType::try_from(serial_type)?;
        serial_types.push(serial_type);
        assert!(pos + nr < payload.len());
        pos += nr;
        assert!(header_size >= nr);
        header_size -= nr;
    }
    let mut values = Vec::new();
    for serial_type in serial_types {
        let (value, usize) = read_value(&payload[pos..], serial_type)?;
        pos += usize;
        values.push(value);
    }
    Ok(Record { _values: values })
}

pub fn read_value(buf: &[u8], serial_type: SerialType) -> Result<(Value, usize)> {
    match serial_type {
        SerialType::Null => Ok((Value::Null, 0)),
        SerialType::UInt8 => Ok((Value::Integer(buf[0] as i64), 1)),
        SerialType::BEInt16 => Ok((
            Value::Integer(i16::from_be_bytes([buf[0], buf[1]]) as i64),
            2,
        )),
        SerialType::BEInt24 => Ok((
            Value::Integer(i32::from_be_bytes([0, buf[0], buf[1], buf[2]]) as i64),
            3,
        )),
        SerialType::BEInt32 => Ok((
            Value::Integer(i32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as i64),
            4,
        )),
        SerialType::BEInt48 => Ok((
            Value::Integer(i64::from_be_bytes([
                0, 0, buf[0], buf[1], buf[2], buf[3], buf[4], buf[5],
            ])),
            6,
        )),
        SerialType::BEInt64 => Ok((
            Value::Integer(i64::from_be_bytes([
                buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
            ])),
            8,
        )),
        SerialType::BEFloat64 => Ok((
            Value::Float(f64::from_be_bytes([
                buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
            ])),
            8,
        )),
        SerialType::ConstInt0 => Ok((Value::Integer(0), 0)),
        SerialType::ConstInt1 => Ok((Value::Integer(1), 0)),
        SerialType::Blob(n) => Ok((Value::Blob(buf[0..n].to_vec()), n)),
        SerialType::String(n) => {
            let value = String::from_utf8(buf[0..n].to_vec())?;
            Ok((Value::Text(value), n))
        }
    }
}

pub fn read_varint(buf: &[u8]) -> Result<(u64, usize)> {
    let mut value = 0;
    let mut shift = 0;
    let mut i = 0;
    loop {
        let byte = buf[i];
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        i += 1;
    }
    Ok((value, i + 1))
}