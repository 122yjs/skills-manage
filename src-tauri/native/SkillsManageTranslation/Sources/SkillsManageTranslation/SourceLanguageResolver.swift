import Foundation
import NaturalLanguage

struct SourceLanguageResolver {
    private static let minimumConfidence = 0.5
    /// 짧은 문장은 오탐이 많다. 스킬 설명 한 줄이 엉뚱한 언어로 인식되어
    /// 쓰지 않는 언어 팩 다운로드를 요구하지 않도록 최소 길이를 둔다.
    private static let minimumCharacterCount = 12

    static func resolve(
        sourceText: String,
        requestedSourceLanguage: String?
    ) -> String? {
        if let requested = requestedSourceLanguage?
            .trimmingCharacters(in: .whitespacesAndNewlines),
           !requested.isEmpty {
            return requested.replacingOccurrences(of: "_", with: "-")
        }

        let trimmed = sourceText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count >= minimumCharacterCount else {
            return nil
        }

        let recognizer = NLLanguageRecognizer()
        recognizer.processString(trimmed)

        guard let hypothesis = recognizer
            .languageHypotheses(withMaximum: 1)
            .max(by: { $0.value < $1.value }),
            hypothesis.key.rawValue != "und",
            hypothesis.value >= minimumConfidence
        else {
            return nil
        }

        return hypothesis.key.rawValue
    }

    static func hasSameBaseLanguage(_ lhs: String, _ rhs: String) -> Bool {
        baseLanguage(lhs) == baseLanguage(rhs)
    }

    private static func baseLanguage(_ language: String) -> String {
        language
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "_", with: "-")
            .split(separator: "-", maxSplits: 1)
            .first?
            .lowercased() ?? ""
    }
}
