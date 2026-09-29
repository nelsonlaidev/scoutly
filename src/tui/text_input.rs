use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::text_byte_index;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TextEditCommand {
    Insert(char),
    MoveCharacterLeft,
    MoveCharacterRight,
    MoveWordLeft,
    MoveWordRight,
    MoveToStart,
    MoveToEnd,
    DeleteCharacterBackward,
    DeleteCharacterForward,
    DeleteWordBackward,
    DeleteWhitespaceWordBackward,
    DeleteToStart,
    DeleteToEnd,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TextEditOutcome {
    pub(super) cursor: usize,
    pub(super) value_changed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WordStyle {
    Small,
    WhitespaceDelimited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CharacterClass {
    Whitespace,
    Word,
    Punctuation,
}

pub(super) fn text_edit_command(key: KeyEvent) -> Option<TextEditCommand> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => Some(TextEditCommand::MoveToStart),
        (KeyCode::Char('e'), KeyModifiers::CONTROL) => Some(TextEditCommand::MoveToEnd),
        (KeyCode::Char('b'), KeyModifiers::CONTROL) => Some(TextEditCommand::MoveCharacterLeft),
        (KeyCode::Char('f'), KeyModifiers::CONTROL) => Some(TextEditCommand::MoveCharacterRight),
        (KeyCode::Char('h'), KeyModifiers::CONTROL) => {
            Some(TextEditCommand::DeleteCharacterBackward)
        }
        (KeyCode::Char('b'), KeyModifiers::ALT)
        | (KeyCode::Left, KeyModifiers::CONTROL | KeyModifiers::ALT) => {
            Some(TextEditCommand::MoveWordLeft)
        }
        (KeyCode::Char('f'), KeyModifiers::ALT)
        | (KeyCode::Right, KeyModifiers::CONTROL | KeyModifiers::ALT) => {
            Some(TextEditCommand::MoveWordRight)
        }
        (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
            Some(TextEditCommand::DeleteWhitespaceWordBackward)
        }
        (KeyCode::Backspace, KeyModifiers::CONTROL | KeyModifiers::ALT) => {
            Some(TextEditCommand::DeleteWordBackward)
        }
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => Some(TextEditCommand::DeleteToStart),
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => Some(TextEditCommand::DeleteToEnd),
        (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
            Some(TextEditCommand::DeleteCharacterForward)
        }
        (KeyCode::Left, KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            Some(TextEditCommand::MoveCharacterLeft)
        }
        (KeyCode::Right, KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            Some(TextEditCommand::MoveCharacterRight)
        }
        (KeyCode::Home, KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            Some(TextEditCommand::MoveToStart)
        }
        (KeyCode::End, KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            Some(TextEditCommand::MoveToEnd)
        }
        (KeyCode::Backspace, KeyModifiers::NONE) => Some(TextEditCommand::DeleteCharacterBackward),
        (KeyCode::Delete, KeyModifiers::NONE) => Some(TextEditCommand::DeleteCharacterForward),
        (KeyCode::Char(character), KeyModifiers::NONE | KeyModifiers::SHIFT)
            if !character.is_control() =>
        {
            Some(TextEditCommand::Insert(character))
        }
        _ => None,
    }
}

pub(super) fn apply_text_edit(
    value: &mut String,
    cursor: usize,
    command: TextEditCommand,
) -> TextEditOutcome {
    let character_count = value.chars().count();
    let cursor = cursor.min(character_count);

    match command {
        TextEditCommand::Insert(character) => {
            let byte = text_byte_index(value, cursor);
            value.insert(byte, character);
            TextEditOutcome {
                cursor: cursor + 1,
                value_changed: true,
            }
        }
        TextEditCommand::MoveCharacterLeft => unchanged(cursor.saturating_sub(1)),
        TextEditCommand::MoveCharacterRight => unchanged((cursor + 1).min(character_count)),
        TextEditCommand::MoveWordLeft => {
            unchanged(word_boundary_left(value, cursor, WordStyle::Small))
        }
        TextEditCommand::MoveWordRight => {
            unchanged(word_boundary_right(value, cursor, WordStyle::Small))
        }
        TextEditCommand::MoveToStart => unchanged(0),
        TextEditCommand::MoveToEnd => unchanged(character_count),
        TextEditCommand::DeleteCharacterBackward => {
            let start = cursor.saturating_sub(1);
            remove_range(value, start, cursor)
        }
        TextEditCommand::DeleteCharacterForward => {
            remove_range(value, cursor, (cursor + 1).min(character_count))
        }
        TextEditCommand::DeleteWordBackward => {
            let start = word_boundary_left(value, cursor, WordStyle::Small);
            remove_range(value, start, cursor)
        }
        TextEditCommand::DeleteWhitespaceWordBackward => {
            let start = word_boundary_left(value, cursor, WordStyle::WhitespaceDelimited);
            remove_range(value, start, cursor)
        }
        TextEditCommand::DeleteToStart => remove_range(value, 0, cursor),
        TextEditCommand::DeleteToEnd => remove_range(value, cursor, character_count),
    }
}

const fn unchanged(cursor: usize) -> TextEditOutcome {
    TextEditOutcome {
        cursor,
        value_changed: false,
    }
}

fn remove_range(value: &mut String, start: usize, end: usize) -> TextEditOutcome {
    if start >= end {
        return unchanged(start);
    }

    let start_byte = text_byte_index(value, start);
    let end_byte = text_byte_index(value, end);
    value.replace_range(start_byte..end_byte, "");

    TextEditOutcome {
        cursor: start,
        value_changed: true,
    }
}

fn word_boundary_left(value: &str, cursor: usize, style: WordStyle) -> usize {
    let characters = value.chars().collect::<Vec<_>>();
    let mut position = cursor.min(characters.len());

    while position > 0
        && character_class(characters[position - 1], style) == CharacterClass::Whitespace
    {
        position -= 1;
    }

    let Some(target) = position
        .checked_sub(1)
        .map(|index| character_class(characters[index], style))
    else {
        return 0;
    };

    while position > 0 && character_class(characters[position - 1], style) == target {
        position -= 1;
    }

    position
}

fn word_boundary_right(value: &str, cursor: usize, style: WordStyle) -> usize {
    let characters = value.chars().collect::<Vec<_>>();
    let mut position = cursor.min(characters.len());

    while position < characters.len()
        && character_class(characters[position], style) == CharacterClass::Whitespace
    {
        position += 1;
    }

    let Some(character) = characters.get(position) else {
        return position;
    };
    let target = character_class(*character, style);

    while position < characters.len() && character_class(characters[position], style) == target {
        position += 1;
    }

    position
}

fn character_class(character: char, style: WordStyle) -> CharacterClass {
    if character.is_whitespace() {
        CharacterClass::Whitespace
    } else if style == WordStyle::WhitespaceDelimited
        || character.is_alphanumeric()
        || character == '_'
    {
        CharacterClass::Word
    } else {
        CharacterClass::Punctuation
    }
}

#[cfg(test)]
pub(super) struct TextEditCase {
    pub(super) name: &'static str,
    pub(super) value: &'static str,
    pub(super) cursor: usize,
    pub(super) code: KeyCode,
    pub(super) modifiers: KeyModifiers,
    pub(super) command: TextEditCommand,
    pub(super) expected_value: &'static str,
    pub(super) expected_cursor: usize,
}

#[cfg(test)]
pub(super) const TEXT_EDIT_CASES: &[TextEditCase] = &[
    TextEditCase {
        name: "move to start",
        value: "one two",
        cursor: 4,
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::CONTROL,
        command: TextEditCommand::MoveToStart,
        expected_value: "one two",
        expected_cursor: 0,
    },
    TextEditCase {
        name: "move to end",
        value: "one two",
        cursor: 1,
        code: KeyCode::Char('e'),
        modifiers: KeyModifiers::CONTROL,
        command: TextEditCommand::MoveToEnd,
        expected_value: "one two",
        expected_cursor: 7,
    },
    TextEditCase {
        name: "move one character left",
        value: "one two",
        cursor: 4,
        code: KeyCode::Char('b'),
        modifiers: KeyModifiers::CONTROL,
        command: TextEditCommand::MoveCharacterLeft,
        expected_value: "one two",
        expected_cursor: 3,
    },
    TextEditCase {
        name: "move one character right",
        value: "one two",
        cursor: 4,
        code: KeyCode::Char('f'),
        modifiers: KeyModifiers::CONTROL,
        command: TextEditCommand::MoveCharacterRight,
        expected_value: "one two",
        expected_cursor: 5,
    },
    TextEditCase {
        name: "readline backspace",
        value: "one two",
        cursor: 4,
        code: KeyCode::Char('h'),
        modifiers: KeyModifiers::CONTROL,
        command: TextEditCommand::DeleteCharacterBackward,
        expected_value: "onetwo",
        expected_cursor: 3,
    },
    TextEditCase {
        name: "move one word left",
        value: "one two",
        cursor: 7,
        code: KeyCode::Char('b'),
        modifiers: KeyModifiers::ALT,
        command: TextEditCommand::MoveWordLeft,
        expected_value: "one two",
        expected_cursor: 4,
    },
    TextEditCase {
        name: "move one word right",
        value: "one two",
        cursor: 0,
        code: KeyCode::Char('f'),
        modifiers: KeyModifiers::ALT,
        command: TextEditCommand::MoveWordRight,
        expected_value: "one two",
        expected_cursor: 3,
    },
    TextEditCase {
        name: "move one word left with control arrow",
        value: "one two",
        cursor: 7,
        code: KeyCode::Left,
        modifiers: KeyModifiers::CONTROL,
        command: TextEditCommand::MoveWordLeft,
        expected_value: "one two",
        expected_cursor: 4,
    },
    TextEditCase {
        name: "move one word right with control arrow",
        value: "one two",
        cursor: 0,
        code: KeyCode::Right,
        modifiers: KeyModifiers::CONTROL,
        command: TextEditCommand::MoveWordRight,
        expected_value: "one two",
        expected_cursor: 3,
    },
    TextEditCase {
        name: "move one word left with alt arrow",
        value: "one two",
        cursor: 7,
        code: KeyCode::Left,
        modifiers: KeyModifiers::ALT,
        command: TextEditCommand::MoveWordLeft,
        expected_value: "one two",
        expected_cursor: 4,
    },
    TextEditCase {
        name: "move one word right with alt arrow",
        value: "one two",
        cursor: 0,
        code: KeyCode::Right,
        modifiers: KeyModifiers::ALT,
        command: TextEditCommand::MoveWordRight,
        expected_value: "one two",
        expected_cursor: 3,
    },
    TextEditCase {
        name: "move left with shift",
        value: "one two",
        cursor: 4,
        code: KeyCode::Left,
        modifiers: KeyModifiers::SHIFT,
        command: TextEditCommand::MoveCharacterLeft,
        expected_value: "one two",
        expected_cursor: 3,
    },
    TextEditCase {
        name: "move right with shift",
        value: "one two",
        cursor: 4,
        code: KeyCode::Right,
        modifiers: KeyModifiers::SHIFT,
        command: TextEditCommand::MoveCharacterRight,
        expected_value: "one two",
        expected_cursor: 5,
    },
    TextEditCase {
        name: "move home with shift",
        value: "one two",
        cursor: 4,
        code: KeyCode::Home,
        modifiers: KeyModifiers::SHIFT,
        command: TextEditCommand::MoveToStart,
        expected_value: "one two",
        expected_cursor: 0,
    },
    TextEditCase {
        name: "move end with shift",
        value: "one two",
        cursor: 4,
        code: KeyCode::End,
        modifiers: KeyModifiers::SHIFT,
        command: TextEditCommand::MoveToEnd,
        expected_value: "one two",
        expected_cursor: 7,
    },
    TextEditCase {
        name: "delete whitespace-delimited word",
        value: "one two",
        cursor: 7,
        code: KeyCode::Char('w'),
        modifiers: KeyModifiers::CONTROL,
        command: TextEditCommand::DeleteWhitespaceWordBackward,
        expected_value: "one ",
        expected_cursor: 4,
    },
    TextEditCase {
        name: "delete word with control backspace",
        value: "one two",
        cursor: 7,
        code: KeyCode::Backspace,
        modifiers: KeyModifiers::CONTROL,
        command: TextEditCommand::DeleteWordBackward,
        expected_value: "one ",
        expected_cursor: 4,
    },
    TextEditCase {
        name: "delete word with alt backspace",
        value: "one two",
        cursor: 7,
        code: KeyCode::Backspace,
        modifiers: KeyModifiers::ALT,
        command: TextEditCommand::DeleteWordBackward,
        expected_value: "one ",
        expected_cursor: 4,
    },
    TextEditCase {
        name: "delete to start",
        value: "one two",
        cursor: 4,
        code: KeyCode::Char('u'),
        modifiers: KeyModifiers::CONTROL,
        command: TextEditCommand::DeleteToStart,
        expected_value: "two",
        expected_cursor: 0,
    },
    TextEditCase {
        name: "delete to end",
        value: "one two",
        cursor: 4,
        code: KeyCode::Char('k'),
        modifiers: KeyModifiers::CONTROL,
        command: TextEditCommand::DeleteToEnd,
        expected_value: "one ",
        expected_cursor: 4,
    },
    TextEditCase {
        name: "delete character forward",
        value: "one two",
        cursor: 4,
        code: KeyCode::Char('d'),
        modifiers: KeyModifiers::CONTROL,
        command: TextEditCommand::DeleteCharacterForward,
        expected_value: "one wo",
        expected_cursor: 4,
    },
    TextEditCase {
        name: "delete unicode character",
        value: "a例b",
        cursor: 2,
        code: KeyCode::Backspace,
        modifiers: KeyModifiers::NONE,
        command: TextEditCommand::DeleteCharacterBackward,
        expected_value: "ab",
        expected_cursor: 1,
    },
    TextEditCase {
        name: "insert shifted space",
        value: "onetwo",
        cursor: 3,
        code: KeyCode::Char(' '),
        modifiers: KeyModifiers::SHIFT,
        command: TextEditCommand::Insert(' '),
        expected_value: "one two",
        expected_cursor: 4,
    },
    TextEditCase {
        name: "empty input",
        value: "",
        cursor: 0,
        code: KeyCode::Char('k'),
        modifiers: KeyModifiers::CONTROL,
        command: TextEditCommand::DeleteToEnd,
        expected_value: "",
        expected_cursor: 0,
    },
];

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{TEXT_EDIT_CASES, TextEditCommand, apply_text_edit, text_edit_command};

    #[test]
    fn key_events_map_to_commands_and_apply_expected_edits() {
        for case in TEXT_EDIT_CASES {
            assert_eq!(
                text_edit_command(KeyEvent::new(case.code, case.modifiers)),
                Some(case.command),
                "{}",
                case.name
            );

            let mut value = case.value.to_owned();
            let outcome = apply_text_edit(&mut value, case.cursor, case.command);

            assert_eq!(value, case.expected_value, "{}", case.name);
            assert_eq!(outcome.cursor, case.expected_cursor, "{}", case.name);
        }

        for (code, modifiers) in [
            (KeyCode::Char('q'), KeyModifiers::CONTROL),
            (KeyCode::Delete, KeyModifiers::CONTROL),
            (KeyCode::Tab, KeyModifiers::CONTROL),
            (KeyCode::Backspace, KeyModifiers::SHIFT),
            (KeyCode::Char('x'), KeyModifiers::ALT),
        ] {
            assert_eq!(text_edit_command(KeyEvent::new(code, modifiers)), None);
        }
    }

    #[test]
    fn small_words_keep_underscores_and_separate_punctuation() {
        let mut value = "one_two::three four".to_owned();
        let mut cursor = 0;

        for expected in [7, 9, 14, 19] {
            cursor = apply_text_edit(&mut value, cursor, TextEditCommand::MoveWordRight).cursor;
            assert_eq!(cursor, expected);
        }

        for expected in [15, 9, 7, 0] {
            cursor = apply_text_edit(&mut value, cursor, TextEditCommand::MoveWordLeft).cursor;
            assert_eq!(cursor, expected);
        }
    }

    #[test]
    fn whitespace_word_deletion_differs_from_small_word_deletion() {
        let mut small = "https://example.com/path".to_owned();
        let small_cursor = small.chars().count();
        let outcome = apply_text_edit(
            &mut small,
            small_cursor,
            TextEditCommand::DeleteWordBackward,
        );

        assert_eq!(small, "https://example.com/");
        assert!(outcome.value_changed);

        let mut whitespace = "https://example.com/path".to_owned();
        let whitespace_cursor = whitespace.chars().count();
        let outcome = apply_text_edit(
            &mut whitespace,
            whitespace_cursor,
            TextEditCommand::DeleteWhitespaceWordBackward,
        );

        assert!(whitespace.is_empty());
        assert_eq!(outcome.cursor, 0);
    }

    #[test]
    fn edits_preserve_unicode_boundaries_and_report_no_ops() {
        let mut value = "a例b".to_owned();
        let outcome = apply_text_edit(&mut value, 2, TextEditCommand::DeleteCharacterBackward);

        assert_eq!(value, "ab");
        assert_eq!(outcome.cursor, 1);
        assert!(outcome.value_changed);

        let outcome = apply_text_edit(&mut value, 0, TextEditCommand::DeleteCharacterBackward);
        assert_eq!(value, "ab");
        assert_eq!(outcome.cursor, 0);
        assert!(!outcome.value_changed);

        let end = value.chars().count();
        let outcome = apply_text_edit(&mut value, end, TextEditCommand::DeleteToEnd);
        assert_eq!(value, "ab");
        assert!(!outcome.value_changed);
    }
}
