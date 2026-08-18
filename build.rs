fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/logo.ico");
        let _ = res.compile();
    }
}
