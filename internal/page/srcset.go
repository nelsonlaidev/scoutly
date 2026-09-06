package page

import (
	"errors"
	"strconv"
	"strings"
)

type srcsetCandidate struct {
	url        string
	descriptor *string
}

// parseSrcset parses a srcset attribute using the WHATWG candidate parsing
// algorithm. Invalid image candidates are omitted, as they are by browsers.
func parseSrcset(srcset string) []srcsetCandidate {
	candidates := make([]srcsetCandidate, 0)

	const (
		inDescriptor = iota
		inParens
		afterDescriptor
	)

	for position := 0; position < len(srcset); {
		for position < len(srcset) &&
			(isASCIIWhitespace(srcset[position]) || srcset[position] == ',') {
			position++
		}
		if position >= len(srcset) {
			break
		}

		urlStart := position
		for position < len(srcset) && !isASCIIWhitespace(srcset[position]) {
			position++
		}

		candidateURL := srcset[urlStart:position]
		if strings.HasSuffix(candidateURL, ",") {
			candidateURL = strings.TrimRight(candidateURL, ",")
			candidates = append(candidates, srcsetCandidate{url: candidateURL})
			continue
		}

		for position < len(srcset) && isASCIIWhitespace(srcset[position]) {
			position++
		}
		descriptorTextStart := position
		descriptorTextEnd := position
		descriptors := make([]string, 0, 1)
		currentDescriptor := make([]byte, 0, 16)
		state := inDescriptor
		descriptorComplete := false
		flushDescriptor := func() {
			if len(currentDescriptor) == 0 {
				return
			}
			descriptors = append(descriptors, string(currentDescriptor))
			currentDescriptor = currentDescriptor[:0]
		}

		for !descriptorComplete {
			if position >= len(srcset) {
				flushDescriptor()
				descriptorTextEnd = position
				descriptorComplete = true
				continue
			}

			character := srcset[position]
			switch state {
			case inDescriptor:
				switch {
				case isASCIIWhitespace(character):
					flushDescriptor()
					state = afterDescriptor
					position++
				case character == ',':
					descriptorTextEnd = position
					position++
					flushDescriptor()
					descriptorComplete = true
				case character == '(':
					currentDescriptor = append(currentDescriptor, character)
					state = inParens
					position++
				default:
					currentDescriptor = append(currentDescriptor, character)
					position++
				}
			case inParens:
				currentDescriptor = append(currentDescriptor, character)
				if character == ')' {
					state = inDescriptor
				}
				position++
			case afterDescriptor:
				if isASCIIWhitespace(character) {
					position++
					continue
				}
				state = inDescriptor
			}
		}

		descriptorTextEnd = trimASCIIWhitespaceEnd(srcset, descriptorTextStart, descriptorTextEnd)
		descriptorValue := srcset[descriptorTextStart:descriptorTextEnd]

		descriptorError := false
		widthPresent := false
		densityPresent := false
		futureCompatHPresent := false
		for _, candidateDescriptor := range descriptors {
			switch {
			case strings.HasSuffix(candidateDescriptor, "w"):
				valid, zero := isValidSrcsetNonNegativeInteger(candidateDescriptor[:len(candidateDescriptor)-1])
				if !valid || zero || widthPresent || densityPresent {
					descriptorError = true
					continue
				}
				widthPresent = true
			case strings.HasSuffix(candidateDescriptor, "x"):
				valid, negative := isValidSrcsetFloatingPoint(candidateDescriptor[:len(candidateDescriptor)-1])
				if !valid || negative || widthPresent || densityPresent || futureCompatHPresent {
					descriptorError = true
					continue
				}
				densityPresent = true
			case strings.HasSuffix(candidateDescriptor, "h"):
				valid, zero := isValidSrcsetNonNegativeInteger(candidateDescriptor[:len(candidateDescriptor)-1])
				if !valid || zero || densityPresent || futureCompatHPresent {
					descriptorError = true
					continue
				}
				futureCompatHPresent = true
			default:
				descriptorError = true
			}
		}
		if futureCompatHPresent && !widthPresent {
			descriptorError = true
		}
		if descriptorError {
			continue
		}

		var descriptor *string
		if descriptorValue != "" {
			descriptor = new(descriptorValue)
		}
		candidates = append(candidates, srcsetCandidate{
			url:        candidateURL,
			descriptor: descriptor,
		})

		if position < len(srcset) && srcset[position] == ',' {
			position++
		}
	}

	return candidates
}

func isValidSrcsetNonNegativeInteger(value string) (valid, zero bool) {
	if value == "" {
		return false, false
	}

	zero = true
	for position := 0; position < len(value); position++ {
		character := value[position]
		if character < '0' || character > '9' {
			return false, false
		}
		if character != '0' {
			zero = false
		}
	}
	return true, zero
}

func isValidSrcsetFloatingPoint(value string) (valid, negative bool) {
	if value == "" {
		return false, false
	}

	position := 0
	if value[position] == '-' {
		negative = true
		position++
	}

	integerDigits := 0
	for position < len(value) && value[position] >= '0' && value[position] <= '9' {
		integerDigits++
		position++
	}

	if position < len(value) && value[position] == '.' {
		position++
		fractionStart := position
		for position < len(value) && value[position] >= '0' && value[position] <= '9' {
			position++
		}
		if position == fractionStart {
			return false, false
		}
	} else if integerDigits == 0 {
		return false, false
	}

	if position < len(value) && (value[position] == 'e' || value[position] == 'E') {
		position++
		if position < len(value) && (value[position] == '-' || value[position] == '+') {
			position++
		}
		exponentStart := position
		for position < len(value) && value[position] >= '0' && value[position] <= '9' {
			position++
		}
		if position == exponentStart {
			return false, false
		}
	}
	if position != len(value) {
		return false, false
	}

	number, err := strconv.ParseFloat(value, 64)
	if err != nil && !errors.Is(err, strconv.ErrRange) {
		return false, false
	}
	return true, negative && number < 0
}

func trimASCIIWhitespaceEnd(value string, start, end int) int {
	for end > start && isASCIIWhitespace(value[end-1]) {
		end--
	}
	return end
}

func isASCIIWhitespace(character byte) bool {
	switch character {
	case '\t', '\n', '\f', '\r', ' ':
		return true
	default:
		return false
	}
}
