/// 문자열을 최대 `max_bytes` 바이트까지만 자른다.
///
/// UTF-8 글자 중간에서 자르면 패닉이 나므로, 글자 경계인 위치까지 뒤로 물러난다.
pub fn truncate_at_char_boundary(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::truncate_at_char_boundary;

    #[test]
    fn chinese_error_body_over_200_bytes_is_truncated_on_a_char_boundary() {
        let body = "错误".repeat(101);
        assert_eq!(truncate_at_char_boundary(&body, 200), "错误".repeat(33));
    }
}
