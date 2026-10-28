import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import '../../core/widgets/ios.dart';

/// Branch and uncommitted change of the folder an employee's conversation works
/// in ("⎇ main +12 −3"), as on the desktop. Tap for the diff, long-press to
/// copy the branch. Hidden outside a Git repository.
class GitChip extends StatefulWidget {
  const GitChip({super.key, required this.conversationId});

  final String conversationId;

  /// How often the chip asks again while it is on screen (the desktop's pace).
  static const poll = Duration(seconds: 5);

  @override
  State<GitChip> createState() => _GitChipState();
}

class _GitChipState extends State<GitChip> {
  GitStatusView? _status;
  Timer? _timer;
  StreamSubscription<void>? _online;
  bool _loading = false;

  /// No read access to the status: stop asking.
  bool _unsupported = false;

  @override
  void initState() {
    super.initState();
    final repository = context.read<AureliusRepository>();
    _online = repository.online.listen((_) => _load());
    _timer = Timer.periodic(GitChip.poll, (_) => _load());
    _load();
  }

  @override
  void dispose() {
    _timer?.cancel();
    _online?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    final repository = context.read<AureliusRepository>();
    if (_loading || _unsupported || !repository.linkState.isOnline) return;
    if (WidgetsBinding.instance.lifecycleState case final s? when s != AppLifecycleState.resumed) return;
    _loading = true;
    try {
      final status = await repository.gitStatus(widget.conversationId);
      if (mounted && !(status == null ? _status == null : status.same(_status))) setState(() => _status = status);
    } on ProtocolException {
      // No read access: no chip.
      _unsupported = true;
      _timer?.cancel();
      if (mounted) setState(() => _status = null);
    } catch (_) {
      // Offline for a moment: keep what is shown.
    } finally {
      _loading = false;
    }
  }

  @override
  Widget build(BuildContext context) {
    final status = _status;
    final label = status?.label;
    if (status == null || label == null) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.fromLTRB(12, 6, 12, 0),
      child: Align(
        alignment: Alignment.centerLeft,
        child: Tooltip(
          message: 'Tap for the changes, hold to copy the branch',
          child: Material(
            color: Tokens.surface,
            shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Tokens.radius), side: BorderSide(color: Tokens.border)),
            child: InkWell(
              key: const Key('git-chip'),
              borderRadius: BorderRadius.circular(Tokens.radius),
              onTap: () => GitDiffPage.open(context, widget.conversationId),
              onLongPress: () => copyBranch(context, status),
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 5),
                child: GitSummary(status: status, label: label),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

void copyBranch(BuildContext context, GitStatusView status) {
  Clipboard.setData(ClipboardData(text: status.copyText));
  toast(context, 'Copied ${status.copyText}');
}

/// "⎇ branch +x −y", or "clean".
class GitSummary extends StatelessWidget {
  const GitSummary({super.key, required this.status, required this.label, this.size = 12.5});

  final GitStatusView status;
  final String label;
  final double size;

  @override
  Widget build(BuildContext context) {
    final style = TextStyle(fontSize: size, color: Tokens.text2);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text('⎇', style: style.copyWith(color: Tokens.muted)),
        const SizedBox(width: 6),
        Flexible(child: Text(label, maxLines: 1, overflow: TextOverflow.ellipsis, style: style)),
        const SizedBox(width: 7),
        if (status.clean)
          Text('clean', style: style.copyWith(color: Tokens.muted))
        else ...[
          Text('+${status.insertions}', style: style.copyWith(color: Tokens.accentText)),
          const SizedBox(width: 6),
          Text('−${status.deletions}', style: style.copyWith(color: Tokens.negative)),
        ],
      ],
    );
  }
}

/// The uncommitted change where an employee's conversation works: tracked
/// files as unified diff hunks, new files by name.
class GitDiffPage extends StatefulWidget {
  const GitDiffPage({super.key, required this.conversationId});

  final String conversationId;

  static Future<void> open(BuildContext context, String conversationId) {
    final repository = context.read<AureliusRepository>();
    return Navigator.of(context).push(MaterialPageRoute<void>(
      builder: (_) => RepositoryProvider.value(value: repository, child: GitDiffPage(conversationId: conversationId)),
    ));
  }

  /// Files with more lines than this start folded.
  static const foldAbove = 300;

  @override
  State<GitDiffPage> createState() => _GitDiffPageState();
}

class _GitDiffPageState extends State<GitDiffPage> {
  late Future<GitDiffView?> _diff = _fetch();

  Future<GitDiffView?> _fetch() => context.read<AureliusRepository>().gitDiff(widget.conversationId);

  void _refresh() => setState(() => _diff = _fetch());

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<GitDiffView?>(
      future: _diff,
      builder: (context, snapshot) {
        final diff = snapshot.data;
        final label = diff?.status.label;
        return Scaffold(
          backgroundColor: Tokens.bg,
          appBar: GlassAppBar(
            title: label == null ? const Text('Changes') : GitSummary(status: diff!.status, label: label, size: 15),
            actions: [
              if (diff != null)
                IconButton(tooltip: 'Copy branch', icon: const Icon(Icons.copy_outlined), onPressed: () => copyBranch(context, diff.status)),
              IconButton(tooltip: 'Refresh', icon: const Icon(Icons.refresh), onPressed: _refresh),
            ],
          ),
          body: switch (snapshot) {
            AsyncSnapshot(hasError: true, :final error) => EmptyState('The changes could not be loaded', detail: '$error'),
            AsyncSnapshot(connectionState: ConnectionState.done) when diff == null =>
              const EmptyState('Not a Git repository', detail: 'This conversation works in a folder outside Git.'),
            AsyncSnapshot(connectionState: ConnectionState.done) => RefreshIndicator(
                onRefresh: () async {
                  _refresh();
                  await _diff;
                },
                child: _DiffList(diff: diff!),
              ),
            _ => const Center(child: CircularProgressIndicator()),
          },
        );
      },
    );
  }
}

class _DiffList extends StatelessWidget {
  const _DiffList({required this.diff});

  final GitDiffView diff;

  @override
  Widget build(BuildContext context) {
    final files = diff.fileDiffs;
    final s = diff.status;
    if (files.isEmpty && diff.untracked.isEmpty) {
      return ListView(children: const [SizedBox(height: 120), EmptyState('No uncommitted changes', detail: 'The working tree matches the last commit.')]);
    }
    return ListView.builder(
      padding: const EdgeInsets.only(bottom: 32),
      itemCount: files.length + 2,
      itemBuilder: (context, i) {
        if (i == 0) {
          return Padding(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
            child: Text(
              [
                '${s.files} ${s.files == 1 ? 'file' : 'files'}',
                if (s.head != null) 'against ${s.head}',
                if (diff.truncated) 'diff cut: too large for the phone',
              ].join(' · '),
              style: TextStyle(fontSize: 12.5, color: Tokens.muted),
            ),
          );
        }
        if (i == files.length + 1) return diff.untracked.isEmpty ? const SizedBox.shrink() : _Untracked(paths: diff.untracked);
        return _FileCard(file: files[i - 1]);
      },
    );
  }
}

class _FileCard extends StatefulWidget {
  const _FileCard({required this.file});

  final FileDiff file;

  @override
  State<_FileCard> createState() => _FileCardState();
}

class _FileCardState extends State<_FileCard> {
  late bool _open = widget.file.lines.length <= GitDiffPage.foldAbove;

  @override
  Widget build(BuildContext context) {
    final f = widget.file;
    return Container(
      margin: const EdgeInsets.fromLTRB(12, 8, 12, 0),
      decoration: BoxDecoration(color: Tokens.surface, borderRadius: BorderRadius.circular(Tokens.cardRadius), border: Border.all(color: Tokens.border)),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          InkWell(
            onTap: () => setState(() => _open = !_open),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(12, 10, 8, 10),
              child: Row(
                children: [
                  Expanded(child: Text(f.path, style: TextStyle(fontFamily: Tokens.mono, fontSize: 12.5, color: Tokens.text))),
                  const SizedBox(width: 8),
                  Text('+${f.insertions}', style: Tokens.monoSmall(color: Tokens.accentText)),
                  const SizedBox(width: 6),
                  Text('−${f.deletions}', style: Tokens.monoSmall(color: Tokens.negative)),
                  Icon(_open ? Icons.expand_less : Icons.expand_more, size: 20, color: Tokens.muted),
                ],
              ),
            ),
          ),
          if (_open)
            DecoratedBox(
              decoration: BoxDecoration(border: Border(top: BorderSide(color: Tokens.border))),
              // Every line as wide as the widest (or the card), so the
              // added/removed colours run the full width.
              child: LayoutBuilder(
                builder: (context, box) => SingleChildScrollView(
                  scrollDirection: Axis.horizontal,
                  padding: const EdgeInsets.symmetric(vertical: 6),
                  child: ConstrainedBox(
                    constraints: BoxConstraints(minWidth: box.maxWidth),
                    child: IntrinsicWidth(
                      child: SelectionArea(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [for (final line in f.lines) _line(line)],
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }

  Widget _line(String line) {
    final (color, background) = switch (line) {
      _ when line.startsWith('+') => (Tokens.accentText, Tokens.accentHalo),
      _ when line.startsWith('-') => (Tokens.negative, Tokens.negative.withValues(alpha: 0.10)),
      _ when line.startsWith('@@') => (Tokens.muted, Tokens.surface2),
      _ => (Tokens.text2, null),
    };
    return Container(
      color: background,
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 1),
      child: Text(line.isEmpty ? ' ' : line, softWrap: false, style: TextStyle(fontFamily: Tokens.mono, fontSize: 12, height: 1.35, color: color)),
    );
  }
}

class _Untracked extends StatelessWidget {
  const _Untracked({required this.paths});

  final List<String> paths;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Eyebrow('New files (${paths.length}, not tracked yet)'),
        for (final p in paths)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 3),
            child: Row(
              children: [
                Icon(Icons.add, size: 14, color: Tokens.accentText),
                const SizedBox(width: 8),
                Expanded(child: Text(p, style: TextStyle(fontFamily: Tokens.mono, fontSize: 12.5, color: Tokens.text2))),
              ],
            ),
          ),
      ],
    );
  }
}
