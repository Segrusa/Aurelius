import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/views.dart';
import '../../data/run_config.dart';
import '../chat/chat_view.dart';
import '../company/company_bloc.dart';
import '../runtime/runtime_bloc.dart';

/// "Set for Iris", "From project Atlas", ...: where an effective value comes from.
String runOriginLabel(RunOrigin origin, {required String employee, String? project}) => switch (origin) {
      RunOrigin.employee => 'Set for $employee',
      RunOrigin.project => project == null ? 'From the project' : 'From project $project',
      RunOrigin.global => 'From the default for all employees',
      RunOrigin.connection => 'The connection\'s default',
      RunOrigin.none => 'Not set',
    };

String _model(Resolved<String> r) => r.value == null ? 'Default' : ModelCatalog.friendly(r.value!);

String _effort(Resolved<String> r) => ModelCatalog.effortLabel(r.value);

enum RunField { model, effort }

/// One purpose's model and effort at one scope: what its rows show and what
/// the picker changes.
class RunTarget {
  const RunTarget({required this.scope, required this.purpose, required this.name, required this.resolve, required this.origin, required this.key});

  /// The model and effort of one employee's runs of [purpose].
  factory RunTarget.employee(EmployeeSummary employee, Purpose purpose, {String? project}) => RunTarget(
        scope: RunScope.employee(employee.id),
        purpose: purpose,
        name: switch (purpose) {
          Purpose.subagent => '${employee.name}\'s subagents',
          _ => employee.name,
        },
        resolve: (s) => s.employeeRun(employee.id, purpose: purpose),
        origin: (r, field) => r.fallback
            ? 'Same as ${employee.name}\'s answers'
            : _noneLabel(r, purpose, field) ?? runOriginLabel(r.origin, employee: employee.name, project: project),
        key: '${purpose == Purpose.employee ? 'employee' : purpose.wire}-run',
      );

  /// The default for all employees' runs of [purpose].
  factory RunTarget.global(Purpose purpose) => RunTarget(
        scope: RunScope.global,
        purpose: purpose,
        name: 'all subagents',
        resolve: (s) => s.globalRun(purpose),
        origin: (r, field) => r.fallback
            ? 'Same as the default for all employees'
            : _noneLabel(r, purpose, field) ?? (r.origin == RunOrigin.global ? 'Set for all employees' : runOriginLabel(r.origin, employee: 'all employees')),
        key: 'global-${purpose.wire}-run',
      );

  final RunScope scope;
  final Purpose purpose;

  /// "Iris", "tasks given to Iris", ...: the picker's title ends with it.
  final String name;
  final EmployeeRunView Function(RunConfig) resolve;
  final String Function(Resolved<String>, RunField) origin;

  /// Prefix of the rows' keys ("employee-run" → "employee-run-model").
  final String key;

  /// A subagent model nobody set is Aurelius's own default, not "Not set".
  static String? _noneLabel(Resolved<String> r, Purpose purpose, RunField field) =>
      r.origin == RunOrigin.none && purpose == Purpose.subagent && field == RunField.model ? 'Aurelius\'s default for subagents (Sonnet)' : null;
}

/// Model and effort for one employee's runs: each row shows the effective value
/// and where it comes from; Inherit clears the employee's own layer. With
/// [subagents], the same for its subagents (Edit's Advanced section).
void showEmployeeRunSheet(BuildContext context, EmployeeSummary employee, {bool subagents = false}) {
  final bloc = context.read<RuntimeBloc>();
  final company = context.read<CompanyBloc>();
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    builder: (sheet) => MultiBlocProvider(
      providers: [BlocProvider.value(value: bloc), BlocProvider.value(value: company)],
      child: BlocBuilder<RuntimeBloc, RuntimeState>(
        builder: (context, state) {
          final run = state.employeeRun(employee.id);
          return SafeArea(
            child: ConstrainedBox(
              constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.85),
              child: ListView(
                shrinkWrap: true,
                children: [
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 0, 16, 4),
                    child: Text(subagents ? 'Subagents of ${employee.name}' : 'Model and effort for ${employee.name}', style: Tokens.title(size: 20)),
                  ),
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                    child: Text(
                      [if (run?.connection != null) run!.connection!.displayName, 'applies to new answers'].join(' · '),
                      style: TextStyle(color: Tokens.muted, fontSize: 12.5),
                    ),
                  ),
                  if (run == null)
                    ListTile(title: Text(state.loading ? 'Loading…' : 'Not available while offline.', style: TextStyle(color: Tokens.muted)))
                  else
                    ...runRows(context, state, RunTarget.employee(employee, subagents ? Purpose.subagent : Purpose.employee)),
                  if (!subagents) ListTile(
                    leading: Icon(Icons.tune, color: Tokens.muted),
                    title: const Text('Default for all employees'),
                    onTap: () => showModelSheet(context, Purpose.employee),
                  ),
                ],
              ),
            ),
          );
        },
      ),
    ),
  );
}

/// The Model and Effort rows of [target]: effective value and its origin;
/// a tap opens the picker (subagents: model only). Needs a [RuntimeBloc]
/// above [context].
List<Widget> runRows(BuildContext context, RuntimeState state, RunTarget target) {
  final snapshot = state.snapshot;
  if (snapshot == null) return const [];
  final run = target.resolve(snapshot);
  return [
    ListTile(
      key: ValueKey('${target.key}-model'),
      title: const Text('Model'),
      subtitle: Text(target.origin(run.model, RunField.model), style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
      trailing: Text(_model(run.model), style: TextStyle(color: Tokens.text2, fontSize: 15)),
      onTap: () => _pick(context, target, RunField.model),
    ),
    // Claude Code gives subagents the effort of the run that starts them.
    if (target.purpose == Purpose.subagent)
      ListTile(
        key: ValueKey('${target.key}-effort'),
        title: Text('Effort: follows the employee', style: TextStyle(color: Tokens.muted)),
      )
    else if (run.connection?.supportsEffort ?? true)
      ListTile(
        key: ValueKey('${target.key}-effort'),
        title: const Text('Effort'),
        subtitle: Text(target.origin(run.effort, RunField.effort), style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
        trailing: Text(_effort(run.effort), style: TextStyle(color: Tokens.text2, fontSize: 15)),
        onTap: () => _pick(context, target, RunField.effort),
      ),
  ];
}

/// "Run tasks one at a time": live from the company state.
class EmployeeSequentialTile extends StatelessWidget {
  const EmployeeSequentialTile({super.key, required this.employee});

  final EmployeeSummary employee;

  @override
  Widget build(BuildContext context) {
    final company = context.read<CompanyBloc>();
    final on = context.select<CompanyBloc, bool>((b) => b.state.employee(employee.id)?.sequentialTasks ?? employee.sequentialTasks);
    return SwitchListTile(
      key: const ValueKey('employee-sequential'),
      title: const Text('Run tasks one at a time'),
      subtitle: Text('Tasks wait in line and start one after the other.', style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
      value: on,
      onChanged: (value) => company.add(EmployeeSequentialToggled(employee.id, value)),
    );
  }
}

/// Inherit (with what it would give) plus the connection's models or the effort levels.
void _pick(BuildContext context, RunTarget target, RunField field) {
  final bloc = context.read<RuntimeBloc>();
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    builder: (sheet) => BlocProvider.value(
      value: bloc,
      child: BlocBuilder<RuntimeBloc, RuntimeState>(
        builder: (context, state) {
          final snapshot = state.snapshot;
          if (snapshot == null) return const SizedBox.shrink();
          final run = target.resolve(snapshot);
          // What Inherit gives: the same resolution without this layer's own value.
          final cleared = target.resolve(snapshot.replacing(field == RunField.model ? run.own.withModel(null) : run.own.withEffort(null)));
          final own = field == RunField.model ? run.own.model : run.own.effort;
          final inherited = field == RunField.model ? cleared.model : cleared.effort;
          void choose(String? value) {
            bloc.add(field == RunField.model
                ? RuntimeModelSelected(target.purpose, value, scope: target.scope)
                : RuntimeEffortSelected(target.purpose, value, scope: target.scope));
            Navigator.pop(sheet);
          }

          Widget option(String? value, String title, {String? detail, Key? key}) {
            final selected = own == value;
            return ListTile(
              key: key,
              leading: Icon(selected ? Icons.radio_button_checked : Icons.radio_button_unchecked, color: selected ? Tokens.accentText : Tokens.muted),
              title: Text(title),
              subtitle: detail == null ? null : Text(detail, style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
              onTap: () => choose(value),
            );
          }

          final inheritValue = field == RunField.model ? _model(inherited) : _effort(inherited);
          return SafeArea(
            child: ConstrainedBox(
              constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.8),
              child: ListView(
                shrinkWrap: true,
                children: [
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                    child: Text(field == RunField.model ? 'Model for ${target.name}' : 'Effort for ${target.name}', style: Tokens.title(size: 20)),
                  ),
                  option(null, 'Inherit', detail: '$inheritValue · ${target.origin(inherited, field)}', key: ValueKey('${target.key}-inherit')),
                  if (field == RunField.model) ...[
                    if (run.connection == null || run.connection!.models.isEmpty)
                      ListTile(title: Text('This connection reports no models.', style: TextStyle(color: Tokens.muted))),
                    for (final m in run.connection?.models ?? const <String>[]) option(m, ModelCatalog.friendly(m), detail: ModelCatalog.describe(m)),
                  ] else ...[
                    const Eyebrow('Effort'),
                    for (final (value, label, detail) in ModelCatalog.efforts) option(value, label, detail: detail),
                  ],
                ],
              ),
            ),
          );
        },
      ),
    ),
  );
}
