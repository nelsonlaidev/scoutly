// Package urlutil provides the URL normalization shared by Scoutly's
// discovery pipeline.
package urlutil

import (
	"fmt"
	"net"
	"net/url"
	"strconv"
	"strings"

	"golang.org/x/net/idna"
)

// Normalize returns a copy of input with canonical HTTP scheme, host, path,
// default port, and fragment handling. Non-HTTP URLs retain their original
// host and path.
func Normalize(input *url.URL, keepFragment bool) *url.URL {
	if input == nil {
		return nil
	}

	result := *input
	result.Scheme = strings.ToLower(result.Scheme)
	if isHTTP(result.Scheme) {
		result.Host = CanonicalHost(result.Scheme, result.Host)
		if result.Path == "" {
			result.Path = "/"
		}
	}
	if !keepFragment {
		result.Fragment = ""
		result.RawFragment = ""
	}
	return &result
}

// CanonicalHost lowercases a host and removes its scheme's default port.
func CanonicalHost(scheme, host string) string {
	target := &url.URL{Scheme: strings.ToLower(scheme), Host: strings.ToLower(host)}
	hostname := target.Hostname()
	if !strings.Contains(hostname, ":") {
		if ascii, err := idna.Lookup.ToASCII(hostname); err == nil {
			hostname = ascii
		}
	}
	hostname = strings.ToLower(hostname)

	port := target.Port()
	if (target.Scheme == "http" && port == "80") ||
		(target.Scheme == "https" && port == "443") {
		port = ""
	}
	if port != "" {
		return net.JoinHostPort(hostname, port)
	}
	if strings.Contains(hostname, ":") {
		return "[" + hostname + "]"
	}
	return hostname
}

// SameHost reports whether two URLs have the same canonical host. Scheme is
// intentionally ignored except when normalizing default ports.
func SameHost(left, right *url.URL) bool {
	if left == nil || right == nil {
		return false
	}
	return CanonicalHost(left.Scheme, left.Host) ==
		CanonicalHost(right.Scheme, right.Host)
}

// String returns u's string representation, or an empty string for a nil URL.
func String(u *url.URL) string {
	if u == nil {
		return ""
	}
	return u.String()
}

// IsHTTPWithHost reports whether u is an HTTP(S) URL with a scheme and host.
func IsHTTPWithHost(u *url.URL) bool {
	if u == nil || u.Host == "" {
		return false
	}
	return strings.EqualFold(u.Scheme, "http") || strings.EqualFold(u.Scheme, "https")
}

// ParseHTTP parses value as an HTTP(S) URL with a scheme and host. It returns
// false when value is empty, cannot be parsed, or is not an HTTP(S) URL.
func ParseHTTP(value string) (*url.URL, bool) {
	parsed, err := url.Parse(strings.TrimSpace(value))
	if err != nil || !IsHTTPWithHost(parsed) {
		return nil, false
	}
	return parsed, true
}

// Origin returns u's canonical "scheme://host" origin, or an empty string for
// a nil URL.
func Origin(u *url.URL) string {
	if u == nil {
		return ""
	}
	scheme := strings.ToLower(u.Scheme)
	return scheme + "://" + CanonicalHost(scheme, u.Host)
}

func isHTTP(scheme string) bool {
	return scheme == "http" || scheme == "https"
}

// ParseTarget validates and normalizes a target URL. It returns the
// parsed URL on success, or an error if the input is not a valid
// HTTP(S) URL.
func ParseTarget(value string) (*url.URL, error) {
	target, err := url.Parse(value)
	if err != nil ||
		target.Hostname() == "" ||
		(!strings.EqualFold(target.Scheme, "http") && !strings.EqualFold(target.Scheme, "https")) {
		return nil, fmt.Errorf("target %q must be an HTTP(S) URL", value)
	}

	if port := target.Port(); port != "" {
		number, err := strconv.Atoi(port)
		if err != nil || number < 1 || number > 65535 {
			return nil, fmt.Errorf("target %q has an invalid port", value)
		}
	}

	return Normalize(target, true), nil
}
