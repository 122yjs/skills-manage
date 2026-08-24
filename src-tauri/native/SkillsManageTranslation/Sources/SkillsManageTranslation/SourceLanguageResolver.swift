import Foundation
import NaturalLanguage

struct SourceLanguageResolver {
    private static let minimumConfidence = 0.5

    static func resolve(
        sourceText: String,
        requestedSourceLanguage: String?
    ) -> String? {
        if let requested = requestedSourceLanguage?
            .trimmingCharacters(in: .whitespacesAndNewlines),
           !requested.isEmpty {
            return requested.replacingOccurrences(of: "_", with: "-")
        }

        let recognizer = NLLanguageRecognizer()
        recognizer.processString(sourceText)

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
