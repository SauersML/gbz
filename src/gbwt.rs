//! GBWT: A run-length encoded FM-index storing paths as sequences of node identifiers.
//!
//! The GBWT was originally described in:
//!
//! > Sirén, Garrison, Novak, Paten, Durbin: **Haplotype-aware graph indexes**.  
//! > Bioinformatics, 2020. DOI: [10.1093/bioinformatics/btz575](https://doi.org/10.1093/bioinformatics/btz575)
//!
//! At the moment, this implementation only supports GBWT indexes built with other tools.
//! See also the original [C++ implementation](https://github.com/jltsiren/gbwt).
// FIXME example

use crate::{ENDMARKER, SOURCE_KEY, SOURCE_VALUE};
use crate::bwt::BWT;
use crate::headers::{Header, GBWTPayload};
use crate::support::Tags;
use crate::support;

use simple_sds::serialize::Serialize;
use simple_sds::serialize;

use std::io::{Error, ErrorKind};
use std::io;

//-----------------------------------------------------------------------------

// FIXME tests
/// The GBWT index storing a collection of paths space-efficiently.
///
/// The GBWT stores integer sequences.
/// Each integer is assumed to be a node identifier, and each sequence is interpreted as a path in a graph.
/// If the index is not bidirectional, GBWT node and sequence identifiers correspond directly to node and path identifiers in the original graph.
///
/// In a bidirectional index, each node (path) in the original graph becomes two nodes (sequences) in the GBWT: one for the forward orientation and one for the reverse orientation.
/// A reverse path visits the other orientation of each node on the path in reverse order.
/// The following functions can be used for mapping between the identifiers used by the GBWT and the graph:
///
/// * [`support::encode_node`], [`support::flip_node`], [`support::node_id`], and [`support::node_is_reverse`] for node identifiers.
/// * [`support::encode_path`], [`support::flip_path`], [`support::path_id`], and [`support::path_is_reverse`] for sequence / path identifiers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GBWT {
    header: Header<GBWTPayload>,
    tags: Tags,
    bwt: BWT,
}

// FIXME tests
// Statistics.
impl GBWT {
    /// Returns the total length of the sequences in the index.
    pub fn len(&self) -> usize {
        self.header.payload().size
    }

    /// Returns `true` if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of sequences in the index.
    pub fn sequences(&self) -> usize {
        self.header.payload().sequences
    }

    /// Returns the size of the alphabet.
    pub fn alphabet_size(&self) -> usize {
        self.header.payload().alphabet_size
    }

    /// Returns the alphabet offset for the effective alphabet.
    pub fn alphabet_offset(&self) -> usize {
        self.header.payload().offset
    }

    /// Returns the size of the effective alphabet.
    pub fn effective_size(&self) -> usize {
        self.alphabet_size() - self.alphabet_offset()
    }

    /// Returns the smallest node identifier in the effective alphabet.
    pub fn first_node(&self) -> usize {
        self.alphabet_offset() + 1
    }

    /// Returns `true` if node identifier `id` is in the effective alphabet.
    pub fn has_node(&self, id: usize) -> bool {
        id > self.alphabet_offset() && id < self.alphabet_size()
    }

    /// Returns `true` if the GBWT index is bidirectional.
    pub fn is_bidirectional(&self) -> bool {
        self.header.is_set(GBWTPayload::FLAG_BIDIRECTIONAL)
    }
}

//-----------------------------------------------------------------------------

// FIXME tests
// Sequence navigation.
impl GBWT {
    /// Returns the first position in sequence `sequence`, or [`None`] if no such sequence exists.
    ///
    /// The return value is a pair (node identifier, offset in node).
    pub fn start(&self, sequence: usize) -> Option<(usize, usize)> {
        if let Some(record) = self.bwt.record(ENDMARKER) {
            return record.lf(sequence);
        }
        None
    }

    /// Follows the sequence forward and returns the next position, or [`None`] if no such position exists.
    ///
    /// The argument and the return value are pairs (node identifier, offset in node).
    pub fn forward(&self, pos: (usize, usize)) -> Option<(usize, usize)> {
        // This also catches the endmarker.
        if pos.0 <= self.first_node() {
            return None;
        }
        if let Some(record) = self.bwt.record(pos.0 - self.alphabet_offset()) {
            return record.lf(pos.1);
        }
        None
    }

    /// Follows the sequence backward and returns the previous position, or [`None`] if no such position exists.
    ///
    /// The argument and the return value are pairs (node identifier, offset in node).
    ///
    /// # Panics
    ///
    /// Panics if the index is not bidirectional.
    pub fn backward(&self, pos: (usize, usize)) -> Option<(usize, usize)> {
        if !self.is_bidirectional() {
            panic!("Following sequences backward is only possible in a bidirectional GBWT");
        }
        // This also catches the endmarker.
        if pos.0 <= self.first_node() {
            return None;
        }
        let reverse_id = support::flip_node(pos.0 - self.alphabet_offset());
        if let Some(record) = self.bwt.record(reverse_id) {
            if let Some(predecessor) = record.predecessor_at(pos.1) {
                if let Some(pred_record) = self.bwt.record(predecessor) {
                    if let Some(offset) = pred_record.offset_to(pos) {
                        return Some((predecessor, offset));
                    }
                }
            }
        }
        None
    }

    // FIXME iter(sequence)
}

//-----------------------------------------------------------------------------

// FIXME impl: find, extend, bd_find, extend_forward, extend_backward

//-----------------------------------------------------------------------------

// FIXME tests
impl Serialize for GBWT {
    fn serialize_header<T: io::Write>(&self, writer: &mut T) -> io::Result<()> {
        self.header.serialize(writer)
    }

    fn serialize_body<T: io::Write>(&self, writer: &mut T) -> io::Result<()> {
        self.tags.serialize(writer)?;
        self.bwt.serialize(writer)?;
        serialize::absent_option(writer)?; // Document array samples.
        serialize::absent_option(writer)?; // Metadata. TODO: Support
        Ok(())
    }

    fn load<T: io::Read>(reader: &mut T) -> io::Result<Self> {
        let header = Header::<GBWTPayload>::load(reader)?;
        if let Err(msg) = header.validate() {
            return Err(Error::new(ErrorKind::InvalidData, msg));
        }
        let mut tags = Tags::load(reader)?;
        tags.insert(SOURCE_KEY, SOURCE_VALUE);
        let bwt = BWT::load(reader)?;
        // FIXME we should decompress the endmarker
        serialize::skip_option(reader)?; // Document array samples.
        serialize::skip_option(reader)?; // Metadata. TODO: Support
        Ok(GBWT {
            header: header,
            tags: tags,
            bwt: bwt,
        })
    }

    fn size_in_elements(&self) -> usize {
        self.header.size_in_elements() + self.tags.size_in_elements() + self.bwt.size_in_elements() + 2 * serialize::absent_option_size()
    }
}

//-----------------------------------------------------------------------------

// FIXME SearchState, BDSearchState

//-----------------------------------------------------------------------------

// FIXME Iter

//-----------------------------------------------------------------------------
