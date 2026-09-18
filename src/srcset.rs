#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SrcsetCandidate {
    pub(crate) url: String,
    pub(crate) descriptor: Option<String>,
}

#[derive(Clone, Copy)]
enum ParserState {
    InDescriptor,
    InParens,
    AfterDescriptor,
}

#[derive(Default)]
struct DescriptorState {
    width_present: bool,
    density_present: bool,
    future_compat_height_present: bool,
}

/// Parses a `srcset` attribute with the same candidate-state machine used by
/// browsers. Invalid image candidates are omitted.
pub(crate) fn parse_srcset(srcset: &str) -> Vec<SrcsetCandidate> {
    let bytes = srcset.as_bytes();
    let mut candidates = Vec::new();
    let mut position = 0;

    while position < bytes.len() {
        while position < bytes.len()
            && (is_ascii_whitespace(bytes[position]) || bytes[position] == b',')
        {
            position += 1;
        }

        if position >= bytes.len() {
            break;
        }

        let url_start = position;

        while position < bytes.len() && !is_ascii_whitespace(bytes[position]) {
            position += 1;
        }

        let mut candidate_url = &srcset[url_start..position];

        if candidate_url.ends_with(',') {
            candidate_url = candidate_url.trim_end_matches(',');
            candidates.push(SrcsetCandidate {
                url: candidate_url.to_owned(),
                descriptor: None,
            });
            continue;
        }

        while position < bytes.len() && is_ascii_whitespace(bytes[position]) {
            position += 1;
        }

        let descriptor_text_start = position;
        let mut current_descriptor_start = Some(position);
        let mut state = ParserState::InDescriptor;
        let mut descriptor_state = DescriptorState::default();
        let mut descriptor_error = false;

        let descriptor_text_end = loop {
            if position >= bytes.len() {
                descriptor_error |= !flush_descriptor(
                    srcset,
                    &mut current_descriptor_start,
                    position,
                    &mut descriptor_state,
                );
                break position;
            }

            let character = bytes[position];

            match state {
                ParserState::InDescriptor => match character {
                    value if is_ascii_whitespace(value) => {
                        descriptor_error |= !flush_descriptor(
                            srcset,
                            &mut current_descriptor_start,
                            position,
                            &mut descriptor_state,
                        );
                        state = ParserState::AfterDescriptor;
                        position += 1;
                    }
                    b',' => {
                        let end = position;
                        position += 1;
                        descriptor_error |= !flush_descriptor(
                            srcset,
                            &mut current_descriptor_start,
                            end,
                            &mut descriptor_state,
                        );
                        break end;
                    }
                    b'(' => {
                        state = ParserState::InParens;
                        position += 1;
                    }
                    _ => position += 1,
                },
                ParserState::InParens => {
                    if character == b')' {
                        state = ParserState::InDescriptor;
                    }
                    position += 1;
                }
                ParserState::AfterDescriptor => {
                    if is_ascii_whitespace(character) {
                        position += 1;
                    } else {
                        current_descriptor_start = Some(position);
                        state = ParserState::InDescriptor;
                    }
                }
            }
        };

        let descriptor_text_end =
            trim_ascii_whitespace_end(bytes, descriptor_text_start, descriptor_text_end);
        let descriptor_value = &srcset[descriptor_text_start..descriptor_text_end];

        if descriptor_state.future_compat_height_present && !descriptor_state.width_present {
            descriptor_error = true;
        }

        if descriptor_error {
            continue;
        }

        candidates.push(SrcsetCandidate {
            url: candidate_url.to_owned(),
            descriptor: (!descriptor_value.is_empty()).then(|| descriptor_value.to_owned()),
        });

        if position < bytes.len() && bytes[position] == b',' {
            position += 1;
        }
    }

    candidates
}

fn flush_descriptor(
    srcset: &str,
    start: &mut Option<usize>,
    end: usize,
    state: &mut DescriptorState,
) -> bool {
    let Some(start) = start.take() else {
        return true;
    };
    let descriptor = &srcset[start..end];

    if descriptor.is_empty() {
        return true;
    }

    if let Some(value) = descriptor.strip_suffix('w') {
        let (valid, zero) = valid_non_negative_integer(value);

        if !valid || zero || state.width_present || state.density_present {
            return false;
        }

        state.width_present = true;
    } else if let Some(value) = descriptor.strip_suffix('x') {
        let (valid, negative) = valid_floating_point(value);

        if !valid
            || negative
            || state.width_present
            || state.density_present
            || state.future_compat_height_present
        {
            return false;
        }

        state.density_present = true;
    } else if let Some(value) = descriptor.strip_suffix('h') {
        let (valid, zero) = valid_non_negative_integer(value);

        if !valid || zero || state.density_present || state.future_compat_height_present {
            return false;
        }

        state.future_compat_height_present = true;
    } else {
        return false;
    }
    true
}

fn valid_non_negative_integer(value: &str) -> (bool, bool) {
    if value.is_empty() {
        return (false, false);
    }

    let mut zero = true;

    for character in value.bytes() {
        if !character.is_ascii_digit() {
            return (false, false);
        }
        zero &= character == b'0';
    }

    (true, zero)
}

fn valid_floating_point(value: &str) -> (bool, bool) {
    if value.is_empty() {
        return (false, false);
    }

    let bytes = value.as_bytes();
    let mut position = usize::from(bytes[0] == b'-');

    let integer_start = position;

    while position < bytes.len() && bytes[position].is_ascii_digit() {
        position += 1;
    }

    let integer_digits = position - integer_start;

    if position < bytes.len() && bytes[position] == b'.' {
        position += 1;

        let fraction_start = position;

        while position < bytes.len() && bytes[position].is_ascii_digit() {
            position += 1;
        }

        if position == fraction_start {
            return (false, false);
        }
    } else if integer_digits == 0 {
        return (false, false);
    }

    if position < bytes.len() && matches!(bytes[position], b'e' | b'E') {
        position += 1;

        if position < bytes.len() && matches!(bytes[position], b'-' | b'+') {
            position += 1;
        }

        let exponent_start = position;

        while position < bytes.len() && bytes[position].is_ascii_digit() {
            position += 1;
        }

        if position == exponent_start {
            return (false, false);
        }
    }

    if position != bytes.len() {
        return (false, false);
    }

    value.parse::<f64>().map_or((false, false), |number| {
        (true, value.starts_with('-') && number < 0.0)
    })
}

fn trim_ascii_whitespace_end(bytes: &[u8], start: usize, mut end: usize) -> usize {
    while end > start && is_ascii_whitespace(bytes[end - 1]) {
        end -= 1;
    }
    end
}

const fn is_ascii_whitespace(character: u8) -> bool {
    matches!(character, b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

#[cfg(test)]
mod tests {
    use super::{SrcsetCandidate, parse_srcset, valid_floating_point};

    fn candidate(url: &str, descriptor: Option<&str>) -> SrcsetCandidate {
        SrcsetCandidate {
            url: url.to_owned(),
            descriptor: descriptor.map(str::to_owned),
        }
    }

    #[test]
    fn srcset_table_covers_supported_descriptors() {
        let cases = [
            ("", vec![]),
            (" ,\t, ", vec![]),
            (
                "/small.png 480w, /large.png 960w, /retina.png 2x",
                vec![
                    candidate("/small.png", Some("480w")),
                    candidate("/large.png", Some("960w")),
                    candidate("/retina.png", Some("2x")),
                ],
            ),
            (
                "/one.png,, , /two.png,",
                vec![candidate("/one.png", None), candidate("/two.png", None)],
            ),
            (
                "/one.png,, /two.png calc(100, 200)",
                vec![candidate("/one.png", None)],
            ),
            (
                "/fraction.png .5x, /exponent.png 1e2x, /plain.png",
                vec![
                    candidate("/fraction.png", Some(".5x")),
                    candidate("/exponent.png", Some("1e2x")),
                    candidate("/plain.png", None),
                ],
            ),
            (
                "/empty-width.png w, /fraction-width.png 1.5w, /duplicate-width.png 400w 800w, /zero.png 0w, /negative.png -1x, /signed.png +1x, /trailing-dot.png 1.x, /empty-density.png x, /mixed.png 400w 2x, /valid.png 2x",
                vec![candidate("/valid.png", Some("2x"))],
            ),
            (
                "/height.png 400w 300h, /height-only.png 300h, /duplicate-height.png 400w 200h 100h, /density-height.png 2x 100h, /valid.png 2x",
                vec![
                    candidate("/height.png", Some("400w 300h")),
                    candidate("/valid.png", Some("2x")),
                ],
            ),
            (
                "data:image/png;base64,AAAA 1x, /two.png 2x",
                vec![
                    candidate("data:image/png;base64,AAAA", Some("1x")),
                    candidate("/two.png", Some("2x")),
                ],
            ),
            (
                "\t/one.png\u{000c}1x   ,\r\n/two.png 2x",
                vec![
                    candidate("/one.png", Some("1x")),
                    candidate("/two.png", Some("2x")),
                ],
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(parse_srcset(input), expected, "{input:?}");
        }
    }

    #[test]
    fn floating_point_edges_are_classified_consistently() {
        assert_eq!(valid_floating_point("1e+2"), (true, false));
        assert_eq!(valid_floating_point("1e"), (false, false));
        assert_eq!(valid_floating_point("1z"), (false, false));
        assert_eq!(valid_floating_point("-0"), (true, false));
        assert_eq!(valid_floating_point("1e9999"), (true, false));
    }
}
