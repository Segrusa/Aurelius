import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';

import '../../core/theme/tokens.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import '../artifact/artifact_page.dart';
import '../artifact/artifact_versions.dart';
import 'chat_view.dart';
import '../markdown/links.dart';
import '../markdown/markdown_page.dart' show markdownLinkTap;
import '../markdown/mermaid.dart';
import '../../core/widgets/ios.dart';

/// Files kept with a message, as chips: tap to open.
class MessageFiles extends StatelessWidget {
  const MessageFiles({super.key, required this.files});

  final List<FileRef> files;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: Wrap(
        spacing: 8,
        runSpacing: 8,
        children: [
          for (final f in files)
            ActionChip(
              avatar: Icon(isArtifact(f) ? Icons.web_outlined : f.isImage ? Icons.image_outlined : Icons.description_outlined, size: 18, color: Tokens.accentText),
              label: Text('${f.name} · ${f.sizeLabel}', overflow: TextOverflow.ellipsis),
              // An HTML file opens as a live artifact; Ask for changes only
              // where the chat can be written to.
              onPressed: () => isArtifact(f)
                  ? ArtifactPage.open(context, f, canAsk: context.findAncestorWidgetOfExactType<ChatView>()?.composer ?? false)
                  : FilePage.open(context, f),
            ),
        ],
      ),
    );
  }
}

/// A file kept with a message: an image (pinch to zoom) or text; other kinds
/// are on the PC (their name and size are shown).
class FilePage extends StatefulWidget {
  const FilePage({super.key, required this.file});

  final FileRef file;

  static Future<void> open(BuildContext context, FileRef file) {
    final repository = context.read<AureliusRepository>();
    return Navigator.of(context).push(MaterialPageRoute<void>(
      builder: (_) => RepositoryProvider.value(value: repository, child: FilePage(file: file)),
    ));
  }

  @override
  State<FilePage> createState() => _FilePageState();
}

class _FilePageState extends State<FilePage> {
  late final Future<Uint8List> _bytes = context.read<AureliusRepository>().file(widget.file.id);

  @override
  Widget build(BuildContext context) {
    final f = widget.file;
    return Scaffold(
      backgroundColor: Tokens.bg,
      appBar: GlassAppBar(title: Text(f.name, overflow: TextOverflow.ellipsis)),
      body: FutureBuilder<Uint8List>(
        future: _bytes,
        builder: (context, snapshot) {
          if (snapshot.hasError) {
            return Center(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Text('This file could not be loaded: ${snapshot.error}', textAlign: TextAlign.center, style: TextStyle(color: Tokens.muted)),
              ),
            );
          }
          final bytes = snapshot.data;
          if (bytes == null) return const Center(child: CircularProgressIndicator());
          if (f.isImage) {
            return InteractiveViewer(maxScale: 6, child: Center(child: Image.memory(bytes, key: const Key('file-image'), fit: BoxFit.contain)));
          }
          if (f.isText) {
            final text = utf8.decode(bytes, allowMalformed: true);
            final style = markdownStyle(context);
            final mermaid = mermaidMarkdown(style, selectable: true);
            return f.mediaType == 'text/markdown'
                ? LinkGestures(
                    builder: (context, copied) => Markdown(
                      data: text,
                      selectable: true,
                      styleSheet: style,
                      blockSyntaxes: mermaid.blockSyntaxes,
                      builders: mermaid.builders,
                      padding: const EdgeInsets.all(16),
                      onTapLink: markdownLinkTap(context, null, copied: copied),
                    ),
                  )
                : SingleChildScrollView(padding: const EdgeInsets.all(16), child: SelectableText(text, style: Tokens.monoSmall()));
          }
          return Center(
            child: Padding(
              padding: const EdgeInsets.all(24),
              child: Text('${f.name} (${f.sizeLabel}) can be opened on the PC; the phone shows images and text.', textAlign: TextAlign.center, style: TextStyle(color: Tokens.muted)),
            ),
          );
        },
      ),
    );
  }
}
