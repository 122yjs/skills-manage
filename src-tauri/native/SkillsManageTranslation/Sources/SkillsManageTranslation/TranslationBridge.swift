import AppKit
import Foundation
import SwiftUI
import Translation

typealias TranslationCallback = @convention(c) (
    UnsafePointer<CChar>?,
    UnsafePointer<CChar>?,
    UnsafeMutableRawPointer?
) -> Void

@available(macOS 15.0, *)
private struct TranslationHostView: View {
    let requestID: UInt64
    let sourceText: String
    let sourceLanguage: Locale.Language
    let targetLanguage: Locale.Language

    var body: some View {
        Color.clear
            .frame(width: 0, height: 0)
            .translationTask(source: sourceLanguage, target: targetLanguage) { session in
                do {
                    let response = try await session.translate(sourceText)
                    TranslationCoordinator.shared.finish(
                        requestID: requestID,
                        result: .success(response.targetText)
                    )
                } catch {
                    TranslationCoordinator.shared.finish(
                        requestID: requestID,
                        result: .failure(error)
                    )
                }
            }
    }
}

private final class PendingRequest {
    let callback: TranslationCallback
    let context: UnsafeMutableRawPointer?
    var hostingView: NSView?
    var timeoutWorkItem: DispatchWorkItem?

    init(callback: @escaping TranslationCallback, context: UnsafeMutableRawPointer?) {
        self.callback = callback
        self.context = context
    }
}

@MainActor
private final class TranslationCoordinator {
    static let shared = TranslationCoordinator()

    private var nextRequestID: UInt64 = 1
    private var pendingRequests: [UInt64: PendingRequest] = [:]

    func start(
        sourceText: String,
        sourceLanguage: String?,
        targetLanguage: String,
        parentView: NSView,
        callback: @escaping TranslationCallback,
        context: UnsafeMutableRawPointer?
    ) {
        guard #available(macOS 15.0, *) else {
            invoke(
                callback,
                context: context,
                result: .failure(BridgeError.unsupportedOperatingSystem)
            )
            return
        }

        guard !sourceText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            invoke(callback, context: context, result: .failure(BridgeError.emptyText))
            return
        }

        guard let resolvedSourceLanguage = SourceLanguageResolver.resolve(
            sourceText: sourceText,
            requestedSourceLanguage: sourceLanguage
        ) else {
            invoke(
                callback,
                context: context,
                result: .failure(BridgeError.unableToIdentifyLanguage)
            )
            return
        }

        guard !SourceLanguageResolver.hasSameBaseLanguage(
            resolvedSourceLanguage,
            targetLanguage
        ) else {
            invoke(
                callback,
                context: context,
                result: .failure(BridgeError.sourceMatchesTarget)
            )
            return
        }

        let target = Locale.Language(identifier: targetLanguage)
        let source = Locale.Language(identifier: resolvedSourceLanguage)

        // 번역을 바로 시작하면 언어 팩이 없을 때 macOS가 시스템 다운로드 창을 띄운다.
        // 이미 내려받은 언어일 때만 진행해 화면 로딩 중 창이 뜨지 않게 한다.
        Task { @MainActor in
            let status = await LanguageAvailability().status(from: source, to: target)
            guard status == .installed else {
                self.invoke(
                    callback,
                    context: context,
                    result: .failure(
                        status == .supported
                            ? BridgeError.languageNotDownloaded
                            : BridgeError.unsupportedLanguagePair
                    )
                )
                return
            }

            self.beginTranslation(
                sourceText: sourceText,
                sourceLanguage: source,
                targetLanguage: target,
                parentView: parentView,
                callback: callback,
                context: context
            )
        }
    }

    @available(macOS 15.0, *)
    private func beginTranslation(
        sourceText: String,
        sourceLanguage source: Locale.Language,
        targetLanguage target: Locale.Language,
        parentView: NSView,
        callback: @escaping TranslationCallback,
        context: UnsafeMutableRawPointer?
    ) {
        let requestID = nextRequestID
        nextRequestID &+= 1

        let request = PendingRequest(callback: callback, context: context)
        let host = NSHostingView(
            rootView: TranslationHostView(
                requestID: requestID,
                sourceText: sourceText,
                sourceLanguage: source,
                targetLanguage: target
            )
        )
        host.isHidden = true
        host.frame = .zero
        parentView.addSubview(host)

        request.hostingView = host
        pendingRequests[requestID] = request

        let timeoutWorkItem = DispatchWorkItem { [weak self] in
            self?.finish(
                requestID: requestID,
                result: .failure(BridgeError.timedOut)
            )
        }
        request.timeoutWorkItem = timeoutWorkItem
        DispatchQueue.main.asyncAfter(deadline: .now() + 180, execute: timeoutWorkItem)
    }

    func finish(requestID: UInt64, result: Result<String, Error>) {
        guard let request = pendingRequests.removeValue(forKey: requestID) else {
            return
        }

        request.timeoutWorkItem?.cancel()
        request.timeoutWorkItem = nil
        request.hostingView?.removeFromSuperview()
        request.hostingView = nil
        invoke(request.callback, context: request.context, result: result)
    }

    private func invoke(
        _ callback: TranslationCallback,
        context: UnsafeMutableRawPointer?,
        result: Result<String, Error>
    ) {
        switch result {
        case .success(let translatedText):
            translatedText.withCString { callback($0, nil, context) }
        case .failure(let error):
            let code = BridgeError.code(for: error)
            code.withCString { callback(nil, $0, context) }
        }
    }

    func cancel(context: UnsafeMutableRawPointer?) {
        guard let requestID = pendingRequests.first(where: {
            $0.value.context == context
        })?.key else {
            return
        }

        finish(requestID: requestID, result: .failure(BridgeError.cancelled))
    }
}

private enum BridgeError: Error {
    case unsupportedOperatingSystem
    case emptyText
    case unableToIdentifyLanguage
    case sourceMatchesTarget
    case cancelled
    case timedOut
    case languageNotDownloaded
    case unsupportedLanguagePair

    static func code(for error: Error) -> String {
        if let error = error as? BridgeError {
            switch error {
            case .unsupportedOperatingSystem:
                return "unsupported_os"
            case .emptyText:
                return "empty_text"
            case .unableToIdentifyLanguage:
                return "unable_to_identify_language"
            case .sourceMatchesTarget:
                return "source_matches_target"
            case .cancelled:
                return "cancelled"
            case .timedOut:
                return "translation_timeout"
            case .languageNotDownloaded:
                return "language_not_downloaded"
            case .unsupportedLanguagePair:
                return "unsupported_language_pairing"
            }
        }

        guard #available(macOS 15.0, *) else {
            return "translation_failed"
        }

        switch error {
        case TranslationError.unsupportedSourceLanguage:
            return "unsupported_source_language"
        case TranslationError.unsupportedTargetLanguage:
            return "unsupported_target_language"
        case TranslationError.unsupportedLanguagePairing:
            return "unsupported_language_pairing"
        case TranslationError.unableToIdentifyLanguage:
            return "unable_to_identify_language"
        case TranslationError.nothingToTranslate:
            return "empty_text"
        default:
            return "translation_failed"
        }
    }
}

@MainActor
@_cdecl("skills_manage_cancel_translation")
func cancelTranslation(context: UnsafeMutableRawPointer?) {
    TranslationCoordinator.shared.cancel(context: context)
}

@MainActor
@_cdecl("skills_manage_translate_on_device")
func translateOnDevice(
    sourceText: UnsafePointer<CChar>,
    sourceLanguage: UnsafePointer<CChar>?,
    targetLanguage: UnsafePointer<CChar>,
    parentView: UnsafeMutableRawPointer,
    callback: @escaping TranslationCallback,
    context: UnsafeMutableRawPointer?
) {
    let text = String(cString: sourceText)
    let source = sourceLanguage.map(String.init(cString:))
    let target = String(cString: targetLanguage)
    let view = Unmanaged<NSView>.fromOpaque(parentView).takeUnretainedValue()

    guard #available(macOS 15.0, *) else {
        "unsupported_os".withCString { callback(nil, $0, context) }
        return
    }

    TranslationCoordinator.shared.start(
        sourceText: text,
        sourceLanguage: source,
        targetLanguage: target,
        parentView: view,
        callback: callback,
        context: context
    )
}
