import 'dart:async';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import 'artifact_sheet.dart';

/// Opens a link in the phone's browser; tests replace it.
Future<bool> Function(Uri uri) launchExternal = (uri) => launchUrl(uri, mode: LaunchMode.externalApplication);

/// Opens a page in the system's in-app browser: SFSafariViewController on
/// iOS, Custom Tabs on Android. Never a plain WebView, where Google refuses
/// sign-in; this one keeps the claude.ai login between opens. Tests replace it.
Future<bool> Function(Uri uri) launchInAppBrowser = (uri) => launchUrl(uri, mode: LaunchMode.inAppBrowserView);

const claudeAiLoginUrl = 'https://claude.ai/login';

/// Whether [uri] is a claude.ai page (artifacts, login).
bool isClaudeAiLink(Uri uri) => uri.scheme == 'https' && (uri.host == 'claude.ai' || uri.host.endsWith('.claude.ai'));

Future<bool> _tryLaunch(Future<bool> Function(Uri uri) launch, Uri uri) async {
  try {
    return await launch(uri);
  } on Object {
    return false;
  }
}

/// Open [url]: claude.ai pages in the in-app browser (so the claude.ai login
/// works), others and a failed in-app open in the browser. When that fails
/// too, the link is copied instead.
Future<void> openExternalLink(BuildContext context, String url) async {
  final uri = Uri.tryParse(url);
  var opened = false;
  if (uri != null) {
    if (isClaudeAiLink(uri)) opened = await _tryLaunch(launchInAppBrowser, uri);
    if (!opened) opened = await _tryLaunch(launchExternal, uri);
  }
  if (opened) return;
  await Clipboard.setData(ClipboardData(text: url));
  if (context.mounted) toast(context, 'Could not open the link; it is copied.');
}

/// A page that an `Artifact` tool call published on claude.ai.
class ArtifactPublish {
  const ArtifactPublish({required this.url, required this.title, this.version});

  final String url;
  final String title;
  final int? version;
}

final _artifactUrl = RegExp(r'https://claude\.ai/artifact/[A-Za-z0-9_-]+');

/// Whether this `Artifact` call publishes (no action, or `publish`).
bool isArtifactPublish(ActivityView a) => a.tool == 'Artifact' && (a.field('action').isEmpty || a.field('action') == 'publish');

/// The published page of a finished publish call, read from its result text
/// ("Published … at `url` (Version 2, …)"); the URL it republished to otherwise.
ArtifactPublish? artifactPublished(ActivityView a) {
  if (!isArtifactPublish(a) || a.ok != true) return null;
  final output = a.output ?? '';
  final url = _artifactUrl.firstMatch(output)?.group(0) ?? (a.field('url').isNotEmpty ? a.field('url') : null);
  if (url == null) return null;
  final version = int.tryParse(RegExp(r'\(Version (\d+)').firstMatch(output)?.group(1) ?? '');
  return ArtifactPublish(url: url, title: artifactTitle(a), version: version);
}

/// The title an `Artifact` call gives: its `title`, else the file name without extension.
String artifactTitle(ActivityView a) {
  final title = a.field('title').trim();
  if (title.isNotEmpty) return title;
  final parts = a.field('file_path').split(RegExp(r'[\\/]')).where((p) => p.isNotEmpty);
  if (parts.isEmpty) return 'Artifact';
  final name = parts.last;
  final dot = name.lastIndexOf('.');
  return dot > 0 ? name.substring(0, dot) : name;
}

/// The published page under its tool line: title, View (in the app, when the
/// server kept its file), Open (browser) and Copy link. The title is the one
/// the server lists (the page's own); the call's title or file name until then.
class ArtifactLinkCard extends StatefulWidget {
  const ArtifactLinkCard({super.key, required this.artifact});

  final ArtifactPublish artifact;

  @override
  State<ArtifactLinkCard> createState() => _ArtifactLinkCardState();
}

class _ArtifactLinkCardState extends State<ArtifactLinkCard> {
  AureliusRepository? _repository;
  ArtifactView? _known;
  StreamSubscription<Event>? _published;

  @override
  void initState() {
    super.initState();
    try {
      _repository = context.read<AureliusRepository>();
    } on Object {
      // Without a repository (previews, tests) the card shows the link only.
    }
    // The publish may be recorded just after the card shows.
    _published = _repository?.changes.where((e) => e.kind == EventKind.artifactPublished).listen((_) => _lookUp(fresh: true));
    _lookUp();
  }

  @override
  void dispose() {
    _published?.cancel();
    super.dispose();
  }

  Future<void> _lookUp({bool fresh = false}) async {
    final known = await _repository?.knownArtifact(widget.artifact.url, fresh: fresh);
    if (mounted && known != null) setState(() => _known = known);
  }

  @override
  Widget build(BuildContext context) {
    final artifact = widget.artifact;
    final known = _known;
    final title = known != null && known.title.isNotEmpty ? known.title : artifact.title;
    final version = artifact.version;
    return Container(
      key: const Key('artifact-card'),
      margin: const EdgeInsets.only(top: 6),
      padding: const EdgeInsets.fromLTRB(12, 8, 6, 8),
      decoration: BoxDecoration(
        color: Tokens.surface,
        borderRadius: BorderRadius.circular(Tokens.radius),
        border: Border.all(color: Tokens.border),
      ),
      child: Row(
        children: [
          Icon(Icons.public, size: 18, color: Tokens.accentText),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title, key: const Key('artifact-title'), maxLines: 2, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 14, color: Tokens.text)),
                Text(
                  version == null ? 'Artifact on claude.ai' : 'Artifact on claude.ai · version $version',
                  style: TextStyle(fontSize: 11.5, color: Tokens.muted),
                ),
              ],
            ),
          ),
          if (known != null && known.hasContent)
            TextButton(
              key: const Key('artifact-view'),
              style: TextButton.styleFrom(visualDensity: VisualDensity.compact),
              onPressed: () => openArtifact(context, known),
              child: const Text('View'),
            ),
          TextButton(
            key: const Key('artifact-open'),
            style: TextButton.styleFrom(visualDensity: VisualDensity.compact),
            onPressed: () => openExternalLink(context, artifact.url),
            child: const Text('Open'),
          ),
          CopyButton(key: const Key('artifact-copy'), text: artifact.url, tooltip: 'Copy link'),
        ],
      ),
    );
  }
}

/// Settings → Advanced → MCP & tools: whether runs may publish Claude Code
/// artifacts, read-only (it is switched on the desktop). Hidden until
/// `artifact.get_setting` answers, so older servers show nothing.
class ArtifactsStatus extends StatefulWidget {
  const ArtifactsStatus({super.key});

  @override
  State<ArtifactsStatus> createState() => _ArtifactsStatusState();
}

class _ArtifactsStatusState extends State<ArtifactsStatus> {
  late final AureliusRepository _repository = context.read<AureliusRepository>();
  bool? _enabled;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final setting = await _repository.artifactSetting(const Duration(seconds: 10));
      if (mounted) setState(() => _enabled = setting.enabled);
    } on Object {
      // Older servers do not know artifacts: the block stays hidden.
    }
  }

  @override
  Widget build(BuildContext context) {
    final enabled = _enabled;
    if (enabled == null) return const SizedBox.shrink();
    return Column(
      key: const ValueKey('artifacts-section'),
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const GroupHeader('Claude artifacts', caps: true),
        InsetGroup(children: [
          GroupRow(
            key: const ValueKey('artifacts-status'),
            title: 'Employees may publish pages',
            value: enabled ? 'On' : 'Off',
          ),
        ]),
        Padding(
          padding: const EdgeInsets.fromLTRB(32, 6, 32, 8),
          child: Text(
            'Switched in Aurelius on the desktop. Pages go to claude.ai; links to them in a chat still open here.',
            style: TextStyle(color: Tokens.label3, fontSize: 13),
          ),
        ),
      ],
    );
  }
}

/// Settings: sign in to claude.ai in the in-app browser, so artifact pages
/// (Docs, Slides, Design, Dashboard) open from their cards. The phone keeps
/// that login; Aurelius never reads it.
class ClaudeAiAccountSection extends StatelessWidget {
  const ClaudeAiAccountSection({super.key});

  @override
  Widget build(BuildContext context) {
    return Column(
      key: const ValueKey('claude-ai-section'),
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const GroupHeader('claude.ai', caps: true),
        InsetGroup(children: [
          GroupRow(
            key: const ValueKey('claude-ai-account'),
            title: 'claude.ai account',
            subtitle: 'Opens artifact pages',
            trailing: TextButton(
              key: const ValueKey('claude-ai-sign-in'),
              onPressed: () => openExternalLink(context, claudeAiLoginUrl),
              child: const Text('Sign in'),
            ),
          ),
        ]),
        Padding(
          padding: const EdgeInsets.fromLTRB(32, 6, 32, 8),
          child: Text(
            'Sign in once, with Google or email, in the in-app browser; claude.ai pages then open signed in. '
            'The login stays on this phone and Aurelius never reads it.',
            style: TextStyle(color: Tokens.label3, fontSize: 13),
          ),
        ),
      ],
    );
  }
}
