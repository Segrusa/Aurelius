import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../app/mobile_core.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/refresh.dart';
import '../../core/widgets/widgets.dart';
import '../../data/company.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../company_org/employee_widgets.dart';
import '../project/task_status_mark.dart';
import 'give_work.dart';
import '../account/claude_login.dart' show ClaudeLoginCard;

/// Home: the Board's dashboard. Give work (to the management team by default),
/// the management team, who is at work and what needs the user.
class DashboardPage extends StatelessWidget {
  const DashboardPage({super.key});

  static String _greeting() {
    final h = DateTime.now().hour;
    if (h < 12) return 'Good morning';
    if (h < 18) return 'Good afternoon';
    return 'Good evening';
  }

  @override
  Widget build(BuildContext context) {
    return BlocListener<CompanyBloc, CompanyState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, state) {
        toast(context, state.notice!);
        context.read<CompanyBloc>().add(const CompanyNoticeShown());
      },
      child: BlocBuilder<CompanyBloc, CompanyState>(
        builder: (context, state) {
          final summary = [
            _greeting(),
            if (state.working > 0) '${state.working} working',
            if (state.waiting > 0) '${state.waiting} waiting',
          ].join(' · ');
          final team = state.managementTeam;
          final teamIds = {for (final m in team) m.id};
          final atWork = state.workingEmployees.where((m) => !teamIds.contains(m.id)).toList()..sort(byName);
          final waiting = state.waitingTasks..sort((a, b) => b.updatedAtMs.compareTo(a.updatedAtMs));
          final proposals = state.pendingProposals;
          return LargeTitleScaffold(
            title: 'Home',
            subtitle: summary,
            actions: [GlassButton(tooltip: 'Give work (⌘N)', icon: CupertinoIcons.square_pencil, onPressed: () => giveWork(context))],
            onRefresh: () => untilReloaded(context.read<CompanyBloc>(), () => context.read<CompanyBloc>().add(const CompanyRefreshed()), busy: (s) => s.loading),
            fill: state.loading && state.employees.isEmpty ? const Center(child: CupertinoActivityIndicator()) : null,
            children: [
              const ClaudeLoginCard(),
              const GroupHeader('Give Work'),
              GiveWorkBox(state: state),
              const SizedBox(height: 8),
              if (waiting.isNotEmpty || proposals.isNotEmpty) ...[
                GroupHeader('Needs You', count: waiting.length + proposals.length, action: 'Inbox', onAction: () => context.go('/inbox')),
                InsetGroup(
                  children: [
                    for (final p in proposals.take(3))
                      GroupRow(
                        key: ValueKey('home-proposal-${p.id}'),
                        onTap: () => context.go('/inbox'),
                        leading: const _Dot(),
                        title: p.isInstructions ? 'New instructions for ${p.targetEmployeeName ?? 'an employee'}' : 'Proposed colleague: ${p.name}',
                        subtitle: 'From ${p.proposedByName}',
                      ),
                    for (final t in waiting.take(6))
                      GroupRow(
                        key: ValueKey('home-waiting-${t.id}'),
                        onTap: () => context.push('/project/${t.projectId}/task/${t.id}'),
                        leading: const _Dot(),
                        title: t.title,
                        subtitle: [t.statusLabel, state.project(t.projectId)?.name ?? ''].where((s) => s.isNotEmpty).join(' · '),
                        trailing: TaskStatusMark(t),
                      ),
                  ],
                ),
                const SizedBox(height: 8),
              ],
              GroupHeader('Management Team', action: shownInMobileCore('company') ? 'Org chart' : null, onAction: shownInMobileCore('company') ? () => context.push('/company?view=chart') : null),
              InsetGroup(
                dividerIndent: 70,
                children: [
                  if (team.isEmpty)
                    GroupRow(
                      title: 'Nobody yet',
                      subtitle: shownInMobileCore('company') ? 'Add employees under More › Company.' : 'Add employees on the PC.',
                      chevron: false,
                      onTap: shownInMobileCore('company') ? () => context.push('/company') : null,
                    ),
                  for (final m in team) EmployeeRow(key: ValueKey('team-${m.id}'), employee: m, state: state),
                ],
              ),
              if (atWork.isNotEmpty) ...[
                const SizedBox(height: 8),
                GroupHeader('At Work', count: atWork.length),
                InsetGroup(
                  dividerIndent: 70,
                  children: [for (final m in atWork) EmployeeRow(key: ValueKey('at-work-${m.id}'), employee: m, state: state)],
                ),
              ],
            ],
          );
        },
      ),
    );
  }
}

class _Dot extends StatelessWidget {
  const _Dot();

  @override
  Widget build(BuildContext context) => Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: Tokens.accent));
}

/// "Give work": a title, who leads it (the single root unless changed) and the
/// project. Give makes it a started ticket at once; Details opens the New ticket form.
class GiveWorkBox extends StatefulWidget {
  const GiveWorkBox({super.key, required this.state});

  final CompanyState state;

  @override
  State<GiveWorkBox> createState() => _GiveWorkBoxState();
}

class _GiveWorkBoxState extends State<GiveWorkBox> {
  final _title = TextEditingController();
  String? _lead;
  String? _project;

  CompanyState get _company => widget.state;

  @override
  void initState() {
    super.initState();
    if (LastGiven.hostOf(context) case final host?) LastGiven.load(host).then((_) => mounted ? setState(() {}) : null);
  }

  /// The lead shown: the user's pick, else the last one given work, else the single root of the company.
  String? get _leadId {
    for (final id in [_lead, LastGiven.lead]) {
      if (id != null && _company.employee(id) != null) return id;
    }
    return defaultAssignee(_company.employees);
  }

  /// The project shown: the user's pick, else the last one given work, else the only (or first) open project.
  String? get _projectId {
    for (final id in [_project, LastGiven.project]) {
      if (id != null && _company.openProjects.any((p) => p.id == id)) return id;
    }
    return _company.openProjects.where((p) => p.status == ProjectStatus.active).firstOrNull?.id;
  }

  bool get _ready => _title.text.trim().isNotEmpty && _projectId != null;

  @override
  void dispose() {
    _title.dispose();
    super.dispose();
  }

  Future<void> _give() async {
    if (!_ready) return;
    final title = _title.text.trim();
    _title.clear();
    setState(() {});
    // Not made (B10, offline): the title comes back to change it (QA2 m8).
    if (!await sendGivenWork(context, _projectId!, title, _leadId) && mounted && _title.text.isEmpty) setState(() => _title.text = title);
  }

  Future<void> _details() async {
    final title = _title.text.trim();
    _title.clear();
    setState(() {});
    await giveWork(context, title: title.isEmpty ? null : title, employeeId: _leadId, projectId: _projectId);
  }

  Future<void> _pickLead() async {
    final groups = pickerGroups(_company.employees);
    final picked = await showModalBottomSheet<String>(
      context: context,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.75),
          child: ListView(
            shrinkWrap: true,
            children: [
              for (final g in groups) ...[
                Padding(padding: const EdgeInsets.fromLTRB(16, 16, 16, 4), child: Eyebrow(g.title)),
                for (final m in g.employees)
                  ListTile(
                    key: ValueKey('give-lead-${m.id}'),
                    leading: Monogram(m.monogram, size: 32, active: m.working),
                    title: Text(m.status == LifecycleStatus.paused ? '${m.name} (paused)' : m.name),
                    subtitle: m.roleLine.isEmpty ? null : Text(m.roleLine),
                    trailing: m.id == _leadId ? Icon(Icons.check, color: Tokens.accentText) : null,
                    onTap: () => Navigator.pop(sheet, m.id),
                  ),
              ],
            ],
          ),
        ),
      ),
    );
    if (picked != null) setState(() => _lead = picked);
  }

  Future<void> _pickProject() async {
    final picked = await showModalBottomSheet<String>(
      context: context,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.75),
          child: ListView(
            shrinkWrap: true,
            children: [
              const Padding(padding: EdgeInsets.fromLTRB(16, 16, 16, 4), child: Eyebrow('Project')),
              for (final p in _company.openProjects)
                ListTile(
                  key: ValueKey('give-project-${p.id}'),
                  leading: LetterTile(p.name.isEmpty ? '?' : p.name.characters.first.toUpperCase(), size: 32),
                  title: Text(p.status == ProjectStatus.paused ? '${p.name} (paused)' : p.name),
                  trailing: p.id == _projectId ? Icon(Icons.check, color: Tokens.accentText) : null,
                  onTap: () => Navigator.pop(sheet, p.id),
                ),
            ],
          ),
        ),
      ),
    );
    if (picked != null) setState(() => _project = picked);
  }

  @override
  Widget build(BuildContext context) {
    final lead = _company.employee(_leadId ?? '');
    final project = _company.project(_projectId ?? '');
    return Padding(
      key: const ValueKey('give-work'),
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: Material(
        color: Tokens.cell,
        borderRadius: BorderRadius.circular(Tokens.cardRadius),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(14, 6, 10, 10),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              TextField(
                key: const ValueKey('give-work-title'),
                controller: _title,
                textCapitalization: TextCapitalization.sentences,
                textInputAction: TextInputAction.send,
                maxLength: 160,
                decoration: const InputDecoration(hintText: 'What should be done?', border: InputBorder.none, counterText: ''),
                onChanged: (_) => setState(() {}),
                onSubmitted: (_) => _give(),
              ),
              Wrap(
                spacing: 8,
                runSpacing: 6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  ActionChip(
                    key: const ValueKey('give-work-lead'),
                    avatar: lead == null ? const Icon(CupertinoIcons.person, size: 16) : Monogram(lead.monogram, size: 20),
                    label: Text(lead == null ? 'Choose who leads' : lead.name),
                    onPressed: _pickLead,
                  ),
                  ActionChip(
                    key: const ValueKey('give-work-project'),
                    avatar: const Icon(CupertinoIcons.folder, size: 16),
                    label: Text(project?.name ?? 'Choose a project'),
                    onPressed: _company.openProjects.isEmpty ? null : _pickProject,
                  ),
                ],
              ),
              const SizedBox(height: 6),
              Row(
                children: [
                  TextButton(key: const ValueKey('give-work-details'), onPressed: _details, child: const Text('Details')),
                  const Spacer(),
                  FilledButton(key: const ValueKey('give-work-send'), onPressed: _ready ? _give : null, child: const Text('Give')),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}
