import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import '../../data/links.dart' show isAureliusHref;
import '../../data/markdown_paths.dart';
import '../artifacts/artifacts.dart' show openExternalLink;
import '../links/link_actions.dart' show openAureliusLink;
import 'links.dart';

/// What went wrong reading Markdown, in words.
String markdownError(Object error) {
  if (error is ProtocolException) {
    return switch (error.code) {
      ErrorCode.forbidden => 'You are not allowed to read this file.',
      ErrorCode.notFound => 'This file does not exist (any more) in the work folder.',
      ErrorCode.validation => error.message.isEmpty ? 'This file cannot be shown.' : error.message,
      _ => error.message.isEmpty ? 'The file could not be loaded.' : error.message,
    };
  }
  return error.toString();
}

/// Open a Markdown file of [scope] in the viewer. [path] is relative to the
/// work folder or absolute inside it.
Future<void> openMarkdown(BuildContext context, FileScope scope, String path) {
  final repository = context.read<AureliusRepository>();
  return Navigator.of(context).push(MaterialPageRoute<void>(
    builder: (_) => RepositoryProvider.value(value: repository, child: MarkdownPage(scope: scope, path: path)),
  ));
}

/// Link handler for rendered Markdown: `.md` links open in the viewer, other
/// links open in the browser (copied when that fails). A `null`
/// [scope] leaves `.md` links inert. [copied] (see `LinkGestures`) turns a
/// long-press into a copy.
MarkdownTapLinkCallback markdownLinkTap(BuildContext context, FileScope? scope, {String? from, bool Function(String href)? copied}) {
  return (text, href, title) {
    if (href == null || (copied?.call(href) ?? false)) return;
    // Ours never goes to the browser, valid or not.
    if (isAureliusHref(href)) {
      openAureliusLink(context, href);
    } else if (isMarkdownPath(href)) {
      if (scope != null) openMarkdown(context, scope, resolveMarkdownPath(href, from: from));
    } else if (!href.startsWith('#')) {
      openExternalLink(context, href);
    }
  };
}

class MarkdownPage extends StatefulWidget {
  const MarkdownPage({super.key, required this.scope, required this.path});

  final FileScope scope;
  final String path;

  @override
  State<MarkdownPage> createState() => _MarkdownPageState();
}

class _MarkdownPageState extends State<MarkdownPage> {
  late Future<MarkdownFileContent> _file = _load();

  Future<MarkdownFileContent> _load() {
    final repository = context.read<AureliusRepository>();
    return repository.markdownFile(widget.scope, widget.path);
  }

  Future<void> _reload() async {
    final next = _load();
    setState(() => _file = next);
    try {
      await next;
    } on Object {
      // Shown by the body.
    }
  }

  @override
  Widget build(BuildContext context) {
    final name = widget.path.split(RegExp(r'[\\/]')).last;
    return FutureBuilder<MarkdownFileContent>(
      future: _file,
      builder: (context, snapshot) {
        final file = snapshot.data;
        return Scaffold(
          backgroundColor: Tokens.bg,
          appBar: GlassAppBar(
            title: Text(name, overflow: TextOverflow.ellipsis),
            actions: [
              if (file != null)
                IconButton(
                  key: const Key('markdown-copy'),
                  tooltip: 'Copy raw Markdown',
                  icon: const Icon(Icons.copy_all_outlined),
                  onPressed: () {
                    Clipboard.setData(ClipboardData(text: file.text));
                    toast(context, 'Copied.');
                  },
                ),
              IconButton(key: const Key('markdown-reload'), tooltip: 'Reload', icon: const Icon(Icons.refresh), onPressed: _reload),
            ],
          ),
          body: _body(context, snapshot),
        );
      },
    );
  }

  Widget _body(BuildContext context, AsyncSnapshot<MarkdownFileContent> snapshot) {
    if (snapshot.hasError) {
      return RefreshIndicator(
        onRefresh: _reload,
        child: ListView(
          physics: const AlwaysScrollableScrollPhysics(),
          padding: const EdgeInsets.all(24),
          children: [
            const SizedBox(height: 80),
            Icon(Icons.description_outlined, size: 40, color: Tokens.muted),
            const SizedBox(height: 12),
            Text(markdownError(snapshot.error!), key: const Key('markdown-error'), textAlign: TextAlign.center, style: TextStyle(color: Tokens.muted)),
            const SizedBox(height: 4),
            Text(widget.path, textAlign: TextAlign.center, style: Tokens.monoSmall()),
            const SizedBox(height: 16),
            Center(child: OutlinedButton(onPressed: _reload, child: const Text('Try again'))),
          ],
        ),
      );
    }
    final file = snapshot.data;
    if (file == null) return const Center(child: CircularProgressIndicator());
    return RefreshIndicator(
      onRefresh: _reload,
      child: ListView(
        physics: const AlwaysScrollableScrollPhysics(),
        padding: const EdgeInsets.all(16),
        children: [
          Text('${file.path} · ${Fmt.bytes(file.byteLength)} · ${Fmt.ago(file.modifiedMs)}', style: Tokens.monoSmall()),
          const SizedBox(height: 12),
          if (file.text.trim().isEmpty)
            Text('This file is empty.', style: TextStyle(color: Tokens.muted))
          else
            MessageMarkdown(data: file.text, scope: widget.scope, from: file.path, selectable: true),
        ],
      ),
    );
  }
}
