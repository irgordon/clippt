#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensitiveKind {
    PrivateKeyBlock,
    AwsAccessKeyId,
    JwtLike,
    GitHubToken,
    OpenAiApiKey,
}

pub struct PrivacyGuard;

impl PrivacyGuard {
    pub fn classify(text: &str) -> Option<SensitiveKind> {
        let trimmed = text.trim();

        if trimmed.is_empty() {
            return None;
        }

        if looks_like_private_key_block(trimmed) {
            return Some(SensitiveKind::PrivateKeyBlock);
        }

        if contains_aws_access_key_id(trimmed) {
            return Some(SensitiveKind::AwsAccessKeyId);
        }

        if contains_github_token(trimmed) {
            return Some(SensitiveKind::GitHubToken);
        }

        if contains_openai_key(trimmed) {
            return Some(SensitiveKind::OpenAiApiKey);
        }

        if looks_like_jwt(trimmed) {
            return Some(SensitiveKind::JwtLike);
        }

        None
    }
}

fn looks_like_private_key_block(text: &str) -> bool {
    text.contains("-----BEGIN ") && text.contains(" PRIVATE KEY-----")
}

fn contains_aws_access_key_id(text: &str) -> bool {
    token_like_words(text).any(|word| {
        let bytes = word.as_bytes();
        bytes.len() == 20
            && bytes.starts_with(b"AKIA")
            && bytes[4..]
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    })
}

fn contains_github_token(text: &str) -> bool {
    token_like_words(text).any(|word| {
        word.starts_with("ghp_")
            || word.starts_with("gho_")
            || word.starts_with("ghu_")
            || word.starts_with("ghs_")
            || word.starts_with("ghr_")
    })
}

fn contains_openai_key(text: &str) -> bool {
    token_like_words(text).any(|word| word.starts_with("sk-"))
}

fn looks_like_jwt(text: &str) -> bool {
    token_like_words(text).any(|word| {
        let mut parts = word.split('.');
        let (Some(a), Some(b), Some(c), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return false;
        };

        is_base64url_like(a) && is_base64url_like(b) && is_base64url_like(c)
    })
}

fn token_like_words(text: &str) -> impl Iterator<Item = &str> {
    text.split_whitespace()
        .map(|part| part.trim_matches(|c: char| c == '"' || c == '\'' || c == ',' || c == ';'))
}

fn is_base64url_like(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
