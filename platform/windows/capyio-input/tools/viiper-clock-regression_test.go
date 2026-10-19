package dualshock4

import (
	"testing"
	"time"
)

func TestCapyioReportTimestampIsCumulative(t *testing.T) {
	d := &DualShock4{}
	start := time.Unix(1, 0)
	for _, ms := range []int64{0, 4, 8, 12, 400} {
		got := d.reportTimestampAt(start.Add(time.Duration(ms) * time.Millisecond))
		want := uint32(188 + ms*3000/16)
		if got != want { t.Fatalf("at %d ms: got %d want %d", ms, got, want) }
	}
	before := uint16(d.reportTimestampAt(start.Add(348 * time.Millisecond)))
	after := uint16(d.reportTimestampAt(start.Add(352 * time.Millisecond)))
	if after-before != 750 { t.Fatal("USB counter does not preserve elapsed ticks across wrap") }
}
