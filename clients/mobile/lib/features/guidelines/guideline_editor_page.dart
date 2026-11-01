import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../data/guidelines.dart';
import '../../data/line_diff.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../inbox/diff_view.dart';
import 'guideline_common.dart';
import 'guideline_markdown.dart';
import 'guidelines_bloc.dart';

/// What Save does with the title and text: create, update or approve. It
/// throws the server's refusal; the editor stays open with the reason.
typedef GuidelineSave = Future<void> Function(String title, String text);

/// A guideline in a full-screen editor (ui §4.3): the title, the text with
/// Write | Preview, the counter against 8 000 and the largest block a run
/// would get against 16 000 above the keyboard (B4). Save is off over a limit,
/// for an empty title or text, and when nothing changed (GL-02 AC5).
class GuidelineEditorPage extends StatefulWidget {
  const GuidelineEditorPage({
    super.key,
    this.guideline,
    this.fromVersion,
    this.initialTitle = '',
    this.initialText = '',
    this.links = const [],
    this.action = 'Save',
    this.allowUnchanged = false,
    required this.onSave,
  });

  /// The guideline being edited (null: a new one or a proposal for one).
  final GuidelineView? guideline;

  /// The version the text was read at (B6 line, `expected_version`).
  final int? fromVersion;
  final String initialTitle;
  final String initialText;

  /// Where the guideline applies (or would): its block is checked against these.
  final List<GuidelineTarget> links;

  /// "Save", or "Approve" for a proposal.
  final String action;

  /// A proposal may be approved as it is.
  final bool allowUnchanged;
  final GuidelineSave onSave;

  /// Open the editor with the screen's [GuidelinesBloc]; true once saved.
  static Future<bool> open(BuildContext context, GuidelineEditorPage page) async =>
      await Navigator.of(context).push<bool>(MaterialPageRoute(fullscreenDialog: true, builder: (_) => withGuidelines(context, page))) ?? false;

  @override
  State<GuidelineEditorPage> createState() => _GuidelineEditorPageState();
}

enum _View { write, preview }

class _GuidelineEditorPageState extends State<GuidelineEditorPage> {
  late final _title = TextEditingController(text: widget.initialTitle);
  late final _text = TextEditingController(text: widget.initialText);
  var _view = _View.write;
  var _saving = false;
  String? _error;
  String? _titleError;

  /// Per context the block without this guideline; the live block adds the text.
  List<(BlockContext, int)> _without = const [];

  @override
  void initState() {
    super.initState();
    unawaited(_loadBlocks());
  }

  Future<void> _loadBlocks() async {
    if (widget.links.isEmpty) return;
    final bloc = context.read<GuidelinesBloc>();
    final company = context.read<CompanyBloc>().state;
    final contexts = contextsOfAll(widget.links, company.employees, company.projects);
    final without = await bloc.sizes.blocks(contexts, widget.guideline?.id ?? '', null);
    if (mounted) setState(() => _without = without);
  }

  @override
  void dispose() {
    _title.dispose();
    _text.dispose();
    super.dispose();
  }

  int get _count => charCount(_text.text);

  LargestBlock? get _largest => largestOf([for (final (ctx, n) in _without) (ctx, n + _count)]);

  bool get _changed => _title.text.trim() != widget.initialTitle.trim() || _text.text != widget.initialText;

  String? get _blocked {
    if (_title.text.trim().isEmpty) return 'Give the guideline a title.';
    if (_text.text.trim().isEmpty) return 'An empty guideline cannot be saved. Archive it instead.';
    if (_count > guidelineMaxChars) return limitNote(_count);
    final big = _largest;
    if (big != null && big.chars > blockMaxChars) {
      final names = Names.of(context);
      return tooLargeLine(
        GuidelineTooLarge(chars: big.chars, limit: blockMaxChars, employeeId: big.context.employeeId, projectId: big.context.projectId),
        employee: names.employee,
        project: names.project,
      );
    }
    return null;
  }

  Future<bool> _leave() async {
    if (!_changed || _saving) return true;
    return confirm(context, title: 'Discard your changes?', action: 'Discard', danger: true);
  }

  Future<void> _save() async {
    final title = _title.text.trim();
    setState(() {
      _saving = true;
      _error = null;
      _titleError = null;
    });
    try {
      await widget.onSave(title, _text.text);
      if (mounted) Navigator.pop(context, true);
    } on ProtocolException catch (e) {
      if (!mounted) return;
      setState(() => _saving = false);
      if (e.code == ErrorCode.versionConflict && widget.guideline != null) {
        await _conflict();
      } else if (e.code == ErrorCode.guidelineTitleTaken) {
        setState(() => _titleError = Names.of(context).error(e, title: title));
      } else {
        setState(() => _error = Names.of(context).error(e, title: title));
      }
    } on Object catch (e) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _error = Names.of(context).error(e);
      });
    }
  }

  /// Someone saved meanwhile (P-64): never overwrite; Compare or Discard mine.
  Future<void> _conflict() async {
    final g = widget.guideline!;
    final repo = context.read<GuidelinesBloc>().repository;
    GuidelineDetail? latest;
    try {
      latest = await repo.guideline(g.id);
    } on Object {
      latest = null;
    }
    if (!mounted) return;
    final who = latest?.guideline.updatedBy?.username ?? 'Someone';
    final version = latest?.version.version;
    final choice = await showDialog<String>(
      context: context,
      builder: (dialog) => AlertDialog(
        title: Text(version == null ? '$who saved a newer version while you were editing.' : '$who saved version $version while you were editing.'),
        content: const Text('Nothing was overwritten.'),
        actions: [
          TextButton(key: const ValueKey('conflict-discard'), onPressed: () => Navigator.pop(dialog, 'discard'), child: const Text('Discard mine')),
          if (latest != null) FilledButton(key: const ValueKey('conflict-compare'), onPressed: () => Navigator.pop(dialog, 'compare'), child: const Text('Compare')),
        ],
      ),
    );
    if (!mounted) return;
    if (choice == 'discard') {
      Navigator.pop(context, false);
    } else if (choice == 'compare' && latest != null) {
      await showModalBottomSheet<void>(
        context: context,
        isScrollControlled: true,
        showDragHandle: true,
        builder: (sheet) => SafeArea(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
            child: SingleChildScrollView(
              child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                Text('Version $version → your text', style: Tokens.title(size: 17)),
                const SizedBox(height: 10),
                DiffLines(key: const ValueKey('conflict-diff'), lines: lineDiff(latest!.version.text, _text.text), collapsedLines: 200),
              ]),
            ),
          ),
        ),
      );
      if (mounted) setState(() => _error = 'Version $version is the latest now. Copy what you need, then discard yours and edit again.');
    }
  }

  @override
  Widget build(BuildContext context) {
    final g = widget.guideline;
    final blocked = _blocked;
    final canSave = !_saving && blocked == null && (_changed || widget.allowUnchanged);
    final names = Names.of(context);
    final largest = _largest;
    final title = g == null ? (widget.action == 'Save' ? 'New guideline' : 'Proposed guideline') : '${g.title} · v${widget.fromVersion ?? g.version}';
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (popped, _) async {
        if (popped) return;
        if (await _leave() && context.mounted) Navigator.pop(context, false);
      },
      child: Scaffold(
        appBar: AppBar(
          title: Text(title, overflow: TextOverflow.ellipsis),
          leading: IconButton(
            tooltip: 'Cancel',
            icon: const Icon(Icons.close),
            onPressed: () async {
              if (await _leave() && context.mounted) Navigator.pop(context, false);
            },
          ),
          actions: [
            Padding(
              padding: const EdgeInsets.only(right: 12),
              child: FilledButton(key: const ValueKey('guideline-save'), onPressed: canSave ? _save : null, child: Text(widget.action)),
            ),
          ],
        ),
        body: SafeArea(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                TextField(
                  key: const ValueKey('guideline-title'),
                  controller: _title,
                  maxLength: 120,
                  textInputAction: TextInputAction.next,
                  decoration: InputDecoration(labelText: 'Title', counterText: '', errorText: _titleError),
                  onChanged: (_) => setState(() => _titleError = null),
                ),
                const SizedBox(height: 8),
                Align(
                  alignment: Alignment.centerLeft,
                  child: PillSegments<_View>(
                    segments: const [(_View.write, 'Write'), (_View.preview, 'Preview')],
                    selected: _view,
                    onChanged: (v) => setState(() => _view = v),
                  ),
                ),
                const SizedBox(height: 8),
                Expanded(
                  child: _view == _View.write
                      ? TextField(
                          key: const ValueKey('guideline-text'),
                          controller: _text,
                          expands: true,
                          maxLines: null,
                          textAlignVertical: TextAlignVertical.top,
                          keyboardType: TextInputType.multiline,
                          style: TextStyle(fontFamily: Tokens.mono, fontSize: 14),
                          decoration: const InputDecoration(alignLabelWithHint: true, hintText: '## Errors\n- Use thiserror in libraries.'),
                          onChanged: (_) => setState(() => _error = null),
                        )
                      : SingleChildScrollView(key: const ValueKey('guideline-preview'), child: GuidelineMarkdown(_text.text)),
                ),
                const SizedBox(height: 8),
                // Above the keyboard, so they show while typing (ui §4.3).
                LimitBar(key: const ValueKey('guideline-counter'), count: _count),
                if (largest != null) ...[
                  const SizedBox(height: 6),
                  LimitBar(
                    key: const ValueKey('guideline-block'),
                    count: largest.chars,
                    max: blockMaxChars,
                    label: 'Largest block: ${contextName(largest.context, employee: names.employee, project: names.project)} · ',
                  ),
                ],
                // Over 8 000 the counter says it already (Pulse m6): not a second time.
                if ((_error ?? (_changed && blocked != limitNote(_count) ? blocked : null)) case final reason?)
                  Padding(
                    padding: const EdgeInsets.only(top: 6),
                    child: Text(reason, key: const ValueKey('guideline-editor-error'), style: TextStyle(fontSize: 13, color: Tokens.negative)),
                  ),
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: Text(
                    widget.fromVersion == null ? 'Applies from the next run.' : 'Applies from the next run. ${keepsVersionLine(widget.fromVersion!)}',
                    style: TextStyle(fontSize: 12.5, color: Tokens.label2),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
