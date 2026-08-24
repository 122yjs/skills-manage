// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "SkillsManageTranslation",
    platforms: [.macOS(.v11)],
    products: [
        .library(
            name: "SkillsManageTranslation",
            type: .static,
            targets: ["SkillsManageTranslation"]
        )
    ],
    targets: [
        .target(name: "SkillsManageTranslation")
    ]
)
