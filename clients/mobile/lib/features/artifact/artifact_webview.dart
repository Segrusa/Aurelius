import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:webview_flutter/webview_flutter.dart';
import 'package:webview_flutter_android/webview_flutter_android.dart';
import 'package:webview_flutter_wkwebview/webview_flutter_wkwebview.dart';

/// Whether a navigation may go ahead: only the document the page itself just
/// loaded with `loadHtmlString` (an about:blank main frame, once per load).
/// Links, form posts, `window.open`, frames and redirects are all refused.
bool artifactNavigationAllowed({required String url, required bool isMainFrame, required bool expectingLoad}) =>
    expectingLoad && isMainFrame && url == 'about:blank';

/// The platform webview an artifact runs in: JavaScript on, nothing else.
/// There is no JavaScript channel: the page never hears from the artifact.
class ArtifactWebView extends StatefulWidget {
  const ArtifactWebView({super.key, required this.document, required this.load, required this.width});

  /// What [ArtifactBloc] made of the artifact for this load.
  final String document;

  /// A new number loads [document] again (Reload, another version).
  final int load;

  /// The CSS width the artifact is laid out at; the view is scaled to fit.
  final int width;

  @override
  State<ArtifactWebView> createState() => _ArtifactWebViewState();
}

class _ArtifactWebViewState extends State<ArtifactWebView> {
  late final WebViewController _controller = _hardened();

  /// The next main-frame navigation is the document [_load] asked for.
  bool _expectingLoad = false;

  WebViewController _hardened() {
    final platform = WebViewPlatform.instance;
    final params = platform is WebKitWebViewPlatform
        ? WebKitWebViewControllerCreationParams(mediaTypesRequiringUserAction: const {PlaybackMediaTypes.audio, PlaybackMediaTypes.video}, javaScriptCanOpenWindowsAutomatically: false)
        : const PlatformWebViewControllerCreationParams();
    final controller = WebViewController.fromPlatformCreationParams(params, onPermissionRequest: (request) => request.deny())
      ..setJavaScriptMode(JavaScriptMode.unrestricted)
      ..setNavigationDelegate(NavigationDelegate(
        onNavigationRequest: (request) {
          final allowed = artifactNavigationAllowed(url: request.url, isMainFrame: request.isMainFrame, expectingLoad: _expectingLoad);
          if (allowed) _expectingLoad = false;
          return allowed ? NavigationDecision.navigate : NavigationDecision.prevent;
        },
        onPageFinished: (_) => _expectingLoad = false,
      ));
    final native = controller.platform;
    if (native is AndroidWebViewController) {
      native
        ..setAllowFileAccess(false)
        ..setAllowContentAccess(false)
        ..setGeolocationEnabled(false)
        ..setMediaPlaybackRequiresUserGesture(true)
        // No file chooser for an <input type=file>: nothing of the phone goes in.
        ..setOnShowFileSelector((_) async => const <String>[]);
    } else if (native is WebKitWebViewController) {
      native
        ..setAllowsLinkPreview(false)
        ..setInspectable(false);
    }
    return controller;
  }

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void didUpdateWidget(ArtifactWebView old) {
    super.didUpdateWidget(old);
    if (old.load != widget.load || old.document != widget.document) _load();
  }

  void _load() {
    _expectingLoad = true;
    // No base URL: the document is about:blank with an opaque origin of its own.
    _controller.loadHtmlString(widget.document);
  }

  @override
  Widget build(BuildContext context) => LayoutBuilder(builder: (context, box) {
        // Laid out at the chosen width and scaled down to fit, never up.
        final width = widget.width.toDouble();
        final scale = box.maxWidth >= width ? 1.0 : box.maxWidth / width;
        final height = box.maxHeight / scale;
        return ClipRect(
          child: OverflowBox(
            alignment: Alignment.topCenter,
            minWidth: width,
            maxWidth: width,
            minHeight: height,
            maxHeight: height,
            child: Transform.scale(
              scale: scale,
              alignment: Alignment.topCenter,
              child: WebViewWidget(
                key: const Key('artifact-webview'),
                controller: _controller,
                gestureRecognizers: {Factory<OneSequenceGestureRecognizer>(() => EagerGestureRecognizer())},
              ),
            ),
          ),
        );
      });
}
