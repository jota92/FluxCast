package fcdp

import (
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

type vectorFile struct {
	Vectors []struct {
		Name      string `json:"name"`
		Header    struct {
			PacketType      uint8  `json:"packet_type"`
			Flags           uint8  `json:"flags"`
			SessionID       uint64 `json:"session_id"`
			StreamID        uint16 `json:"stream_id"`
			Epoch           uint16 `json:"epoch"`
			Sequence        uint32 `json:"sequence_number"`
			FrameID         uint32 `json:"frame_id"`
			FragmentIndex   uint16 `json:"fragment_index"`
			FragmentCount   uint16 `json:"fragment_count"`
			Priority        uint8  `json:"priority"`
			DeadlineMS      uint16 `json:"deadline_ms"`
		} `json:"header"`
		PayloadHex string `json:"payload_hex"`
		PacketHex  string `json:"packet_hex"`
	} `json:"vectors"`
}

func TestCanonicalVectors(t *testing.T) {
	path := filepath.Join("..", "..", "..", "spec", "test-vectors.json")
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var file vectorFile
	if err := json.Unmarshal(raw, &file); err != nil {
		t.Fatal(err)
	}
	for _, vector := range file.Vectors {
		payload, err := hex.DecodeString(vector.PayloadHex)
		if err != nil {
			t.Fatalf("%s payload: %v", vector.Name, err)
		}
		header := Header{
			PacketType: vector.Header.PacketType, Flags: vector.Header.Flags,
			SessionID: vector.Header.SessionID, StreamID: vector.Header.StreamID,
			Epoch: vector.Header.Epoch, Sequence: vector.Header.Sequence,
			FrameID: vector.Header.FrameID, FragmentIndex: vector.Header.FragmentIndex,
			FragmentCount: vector.Header.FragmentCount, Priority: vector.Header.Priority,
			DeadlineMS: vector.Header.DeadlineMS,
		}
		packet, err := Encode(header, payload)
		if err != nil {
			t.Fatalf("%s encode: %v", vector.Name, err)
		}
		if got := hex.EncodeToString(packet); got != vector.PacketHex {
			t.Fatalf("%s encode mismatch: %s", vector.Name, got)
		}
		decoded, body, err := Decode(packet)
		if err != nil || decoded != header || string(body) != string(payload) {
			t.Fatalf("%s decode mismatch: %v", vector.Name, err)
		}
	}
}
