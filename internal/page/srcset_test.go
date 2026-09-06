package page

import (
	"reflect"
	"testing"
)

func TestParseSrcset(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		srcset string
		want   []srcsetCandidate
	}{
		{
			name:   "empty",
			srcset: "",
			want:   []srcsetCandidate{},
		},
		{
			name:   "only separators",
			srcset: " ,\t, ",
			want:   []srcsetCandidate{},
		},
		{
			name:   "density and width descriptors",
			srcset: "/small.png 480w, /large.png 960w, /retina.png 2x",
			want: []srcsetCandidate{
				{url: "/small.png", descriptor: new("480w")},
				{url: "/large.png", descriptor: new("960w")},
				{url: "/retina.png", descriptor: new("2x")},
			},
		},
		{
			name:   "trailing and repeated commas",
			srcset: "/one.png,, , /two.png,",
			want: []srcsetCandidate{
				{url: "/one.png"},
				{url: "/two.png"},
			},
		},
		{
			name:   "invalid descriptors are ignored",
			srcset: "/one.png,, /two.png calc(100, 200)",
			want: []srcsetCandidate{
				{url: "/one.png"},
			},
		},
		{
			name:   "valid floating-point descriptors",
			srcset: "/fraction.png .5x, /exponent.png 1e2x, /plain.png",
			want: []srcsetCandidate{
				{url: "/fraction.png", descriptor: new(".5x")},
				{url: "/exponent.png", descriptor: new("1e2x")},
				{url: "/plain.png"},
			},
		},
		{
			name:   "invalid and mixed descriptors are ignored",
			srcset: "/empty-width.png w, /fraction-width.png 1.5w, /duplicate-width.png 400w 800w, /zero.png 0w, /negative.png -1x, /signed.png +1x, /trailing-dot.png 1.x, /empty-density.png x, /mixed.png 400w 2x, /valid.png 2x",
			want: []srcsetCandidate{
				{url: "/valid.png", descriptor: new("2x")},
			},
		},
		{
			name:   "future height descriptor requires width",
			srcset: "/height.png 400w 300h, /height-only.png 300h, /duplicate-height.png 400w 200h 100h, /density-height.png 2x 100h, /valid.png 2x",
			want: []srcsetCandidate{
				{url: "/height.png", descriptor: new("400w 300h")},
				{url: "/valid.png", descriptor: new("2x")},
			},
		},
		{
			name:   "data URL comma",
			srcset: "data:image/png;base64,AAAA 1x, /two.png 2x",
			want: []srcsetCandidate{
				{url: "data:image/png;base64,AAAA", descriptor: new("1x")},
				{url: "/two.png", descriptor: new("2x")},
			},
		},
		{
			name:   "all ASCII whitespace",
			srcset: "\t/one.png\f1x   ,\r\n/two.png 2x",
			want: []srcsetCandidate{
				{url: "/one.png", descriptor: new("1x")},
				{url: "/two.png", descriptor: new("2x")},
			},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()

			if got := parseSrcset(test.srcset); !reflect.DeepEqual(got, test.want) {
				t.Errorf("parseSrcset(%q) = %#v, want %#v", test.srcset, got, test.want)
			}
		})
	}
}

func TestSrcsetFloatingPointValidationEdges(t *testing.T) {
	t.Parallel()

	for _, test := range []struct {
		value    string
		valid    bool
		negative bool
	}{
		{value: "1e+2", valid: true},
		{value: "1e", valid: false},
		{value: "1z", valid: false},
	} {
		valid, negative := isValidSrcsetFloatingPoint(test.value)
		if valid != test.valid || negative != test.negative {
			t.Errorf("isValidSrcsetFloatingPoint(%q) = %t, %t", test.value, valid, negative)
		}
	}
}
