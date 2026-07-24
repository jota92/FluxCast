// Package fcdp implements strict FCDP v0.1 packet framing.
package fcdp

import (
	"encoding/binary"
	"errors"
)

const HeaderLen = 37
const MaxDatagram = 1200

type Header struct {
	PacketType, Flags            uint8
	SessionID                    uint64
	StreamID, Epoch              uint16
	Sequence, FrameID            uint32
	FragmentIndex, FragmentCount uint16
	Priority                     uint8
	DeadlineMS                   uint16
}

func crc16(b []byte) uint16 {
	v := uint16(0xffff)
	for _, x := range b {
		v ^= uint16(x) << 8
		for i := 0; i < 8; i++ {
			if v&0x8000 != 0 {
				v = (v << 1) ^ 0x1021
			} else {
				v <<= 1
			}
		}
	}
	return v
}
func Encode(h Header, payload []byte) ([]byte, error) {
	if len(payload)+HeaderLen > MaxDatagram {
		return nil, errors.New("FCDP datagram exceeds 1200-byte budget")
	}
	if h.Priority > 3 || h.FragmentCount == 0 || h.FragmentIndex >= h.FragmentCount {
		return nil, errors.New("invalid FCDP fragment")
	}
	p := make([]byte, HeaderLen+len(payload))
	copy(p, "FC")
	p[2] = 1
	p[3] = h.PacketType
	p[4] = h.Flags
	binary.BigEndian.PutUint64(p[6:], h.SessionID)
	binary.BigEndian.PutUint16(p[14:], h.StreamID)
	binary.BigEndian.PutUint16(p[16:], h.Epoch)
	binary.BigEndian.PutUint32(p[18:], h.Sequence)
	binary.BigEndian.PutUint32(p[22:], h.FrameID)
	binary.BigEndian.PutUint16(p[26:], h.FragmentIndex)
	binary.BigEndian.PutUint16(p[28:], h.FragmentCount)
	p[30] = h.Priority
	binary.BigEndian.PutUint16(p[31:], h.DeadlineMS)
	binary.BigEndian.PutUint16(p[33:], uint16(len(payload)))
	binary.BigEndian.PutUint16(p[35:], crc16(p[:35]))
	copy(p[37:], payload)
	return p, nil
}
func Decode(p []byte) (Header, []byte, error) {
	var h Header
	if len(p) < HeaderLen || string(p[:2]) != "FC" || p[2] != 1 {
		return h, nil, errors.New("invalid FCDP header")
	}
	if binary.BigEndian.Uint16(p[35:]) != crc16(p[:35]) {
		return h, nil, errors.New("invalid FCDP CRC")
	}
	n := int(binary.BigEndian.Uint16(p[33:]))
	if len(p)-HeaderLen != n {
		return h, nil, errors.New("FCDP payload length mismatch")
	}
	h = Header{p[3], p[4], binary.BigEndian.Uint64(p[6:]), binary.BigEndian.Uint16(p[14:]), binary.BigEndian.Uint16(p[16:]), binary.BigEndian.Uint32(p[18:]), binary.BigEndian.Uint32(p[22:]), binary.BigEndian.Uint16(p[26:]), binary.BigEndian.Uint16(p[28:]), p[30], binary.BigEndian.Uint16(p[31:])}
	if h.Priority > 3 || h.FragmentCount == 0 || h.FragmentIndex >= h.FragmentCount {
		return h, nil, errors.New("invalid FCDP fragment")
	}
	return h, p[37:], nil
}
