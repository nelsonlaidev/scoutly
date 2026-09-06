// Package ptrutil provides pointer copying.
package ptrutil

// Clone returns a pointer to a copy of value, or nil when value is nil.
func Clone[T any](value *T) *T {
	if value == nil {
		return nil
	}
	return new(*value)
}
