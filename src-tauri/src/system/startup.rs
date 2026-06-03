pub const BACKGROUND_ARG: &str = "--background";

pub fn is_background_launch_from_env() -> bool {
    is_background_launch_from_args(std::env::args())
}

pub fn is_background_launch_from_args<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == BACKGROUND_ARG)
}

#[cfg(test)]
mod tests {
    use super::is_background_launch_from_args;

    #[test]
    fn background_launch_detects_background_arg() {
        assert!(is_background_launch_from_args(["floe", "--background"]));
    }

    #[test]
    fn manual_launch_defaults_to_visible_mode() {
        assert!(!is_background_launch_from_args(["floe"]));
        assert!(!is_background_launch_from_args(["floe", "--other"]));
    }
}
