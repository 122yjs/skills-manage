fn main() {
    #[cfg(target_os = "macos")]
    {
        // Translation.framework is macOS 15+, while this arm64 application
        // supports macOS 11+. Translation use is guarded separately at runtime
        // so macOS 11–14 can still launch the app.
        swift_rs::SwiftLinker::new("11.0")
            .with_package("SkillsManageTranslation", "native/SkillsManageTranslation")
            .link();

        // macOS 10.15~11에서는 Swift 동시성 런타임을 이 경로에서 찾는다.
        // 이 rpath가 없으면 약한 링크가 null로 남아 번역 작업 시작 시 충돌한다.
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

        // Older macOS versions must still be able to launch the application.
        println!("cargo:rustc-link-arg=-Wl,-weak_framework,Translation");
        println!("cargo:rustc-link-arg=-Wl,-weak_framework,_Translation_SwiftUI");
        println!("cargo:rustc-link-arg=-Wl,-weak_framework,SwiftUI");
    }

    tauri_build::build()
}
