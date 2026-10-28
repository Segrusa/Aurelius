import 'package:equatable/equatable.dart';

import 'protocol/protocol.dart';

/// Where a default is set: global, a project or an employee. Most specific wins:
/// employee → project → global. A small value type over the generated
/// [ConfigScope], which has no equality.
class RunScope extends Equatable {
  const RunScope._(this.kind, this.id);

  static const global = RunScope._('global', null);
  const RunScope.project(String id) : this._('project', id);
  const RunScope.employee(String id) : this._('employee', id);

  factory RunScope.of(ConfigScope scope) => switch (scope) {
        ConfigScopeGlobal() => global,
        ConfigScopeProject(:final id) => RunScope.project(id),
        ConfigScopeEmployee(:final id) => RunScope.employee(id),
      };

  /// global | project | employee
  final String kind;
  final String? id;

  ConfigScope toConfig() => switch (kind) {
        'project' => ConfigScopeProject(id: id!),
        'employee' => ConfigScopeEmployee(id: id!),
        _ => const ConfigScopeGlobal(),
      };

  @override
  List<Object?> get props => [kind, id];
}

extension ConnectionViewRun on ConnectionView {
  /// Effort levels exist for Claude Code only (`--effort`).
  bool get supportsEffort => provider == 'claude_code';
}

/// The run settings for one purpose at one scope. A change touches only
/// model/effort and sends every other field back as is.
class RunDefaults extends Equatable {
  const RunDefaults({required this.purpose, required this.settings, this.scope = RunScope.global});

  final Purpose purpose;
  final RunScope scope;
  final RunSettings settings;

  /// inherit | value | disabled (a missing field is inherit, as the daemon's serde default).
  static String _mode(Object? setting) => switch (setting) {
        SettingValue() || Setting2Value() => 'value',
        SettingDisabled() || Setting2Disabled() => 'disabled',
        _ => 'inherit',
      };

  static String? _value(Object? setting) => switch (setting) {
        SettingValue(:final value) => value,
        Setting2Value(:final value) => value,
        _ => null,
      };

  String mode(String field) => _mode(switch (field) {
        'connection' => settings.connection,
        'model' => settings.model,
        'effort' => settings.effort,
        _ => null,
      });

  String? get connectionId => _value(settings.connection);
  String? get model => _value(settings.model);
  String? get effort => _value(settings.effort);

  static Setting2 _setting(String? value) => value == null ? const Setting2Inherit() : Setting2Value(value: value);

  /// `null` = inherit (the next scope up, or the connection's default).
  RunDefaults withModel(String? model) => RunDefaults(
      purpose: purpose,
      scope: scope,
      settings: RunSettings(connection: settings.connection, effort: settings.effort, model: _setting(model), persona: settings.persona));

  /// `null` = inherit (the next scope up, or the runtime's default).
  RunDefaults withEffort(String? effort) => RunDefaults(
      purpose: purpose,
      scope: scope,
      settings: RunSettings(connection: settings.connection, effort: _setting(effort), model: settings.model, persona: settings.persona));

  @override
  List<Object?> get props => [purpose, scope, settings.toJson().toString()];
}

/// Where an effective value comes from.
enum RunOrigin { employee, project, global, connection, none }

/// One effective value and the scope it comes from.
class Resolved<T> extends Equatable {
  const Resolved(this.value, this.origin, {this.fallback = false});

  final T? value;
  final RunOrigin origin;

  /// Subagent runs: the value is the employee's own (nothing is set for
  /// these runs themselves).
  final bool fallback;

  @override
  List<Object?> get props => [value, origin, fallback];
}

/// What an employee's next runs of one purpose use, resolved as the daemon does
/// (employee → project → global; `disabled` stops the search like a value).
class EmployeeRunView extends Equatable {
  const EmployeeRunView({required this.own, required this.connection, required this.model, required this.effort});

  /// The employee's own layer (all inherit when it has none yet).
  final RunDefaults own;
  final ConnectionView? connection;
  final Resolved<String> model;
  final Resolved<String> effort;

  @override
  List<Object?> get props => [own, connection?.id, model, effort];
}

/// The connections and run defaults as the server reports them, with the
/// resolution of what an employee's runs use.
class RunConfig {
  const RunConfig({required this.connections, required this.defaults, this.scoped = const []});

  factory RunConfig.of(ConnectionsSnapshot snapshot) {
    final defaults = <Purpose, RunDefaults>{};
    final scoped = <RunDefaults>[];
    for (final d in snapshot.defaults) {
      final scope = RunScope.of(d.scope);
      final at = RunDefaults(purpose: d.purpose, scope: scope, settings: d.settings);
      if (scope == RunScope.global) {
        defaults[d.purpose] = at;
      } else {
        scoped.add(at);
      }
    }
    return RunConfig(connections: snapshot.connections, defaults: defaults, scoped: scoped);
  }

  final List<ConnectionView> connections;

  /// The global defaults per purpose.
  final Map<Purpose, RunDefaults> defaults;

  /// Project and employee overrides.
  final List<RunDefaults> scoped;

  RunDefaults? at(RunScope scope, Purpose purpose) =>
      scope == RunScope.global ? defaults[purpose] : scoped.where((d) => d.scope == scope && d.purpose == purpose).firstOrNull;

  /// The config with [next] in place of the defaults at its scope.
  RunConfig replacing(RunDefaults next) => next.scope == RunScope.global
      ? RunConfig(connections: connections, defaults: {...defaults, next.purpose: next}, scoped: scoped)
      : RunConfig(
          connections: connections,
          defaults: defaults,
          scoped: [...scoped.where((d) => d.scope != next.scope || d.purpose != next.purpose), next],
        );

  /// Model and effort for an employee's runs of [purpose] (its own answers or
  /// its subagents), with where each comes from. Employees work company-wide:
  /// [projectId] adds that project's layer (a task there), none its own chat.
  EmployeeRunView employeeRun(String employeeId, {String? projectId, Purpose purpose = Purpose.employee}) =>
      _run(RunScope.employee(employeeId), purpose, employeeId: employeeId, projectId: projectId);

  /// What a project conversation (the project's assistant, no employee) runs
  /// on without its own choice: the project's layer, then the global one.
  EmployeeRunView projectRun(String projectId) => _run(RunScope.project(projectId), Purpose.employee, projectId: projectId);

  /// The default for all employees' runs of [purpose].
  EmployeeRunView globalRun(Purpose purpose) => _run(RunScope.global, purpose);

  List<(RunOrigin, RunDefaults?)> _chain(Purpose purpose, String? employeeId, String? projectId) => [
        if (employeeId != null) (RunOrigin.employee, at(RunScope.employee(employeeId), purpose)),
        if (projectId != null) (RunOrigin.project, at(RunScope.project(projectId), purpose)),
        (RunOrigin.global, defaults[purpose]),
      ];

  /// Subagents run on the employee's connection and effort (Claude Code takes
  /// no subagent effort), but their model has its own default (Sonnet).
  EmployeeRunView _run(RunScope scope, Purpose purpose, {String? employeeId, String? projectId}) {
    final own = at(scope, purpose) ?? RunDefaults(purpose: purpose, scope: scope, settings: const RunSettings());
    final mine = _chain(purpose, employeeId, projectId);
    final base = purpose == Purpose.subagent ? _chain(Purpose.employee, employeeId, projectId) : const <(RunOrigin, RunDefaults?)>[];
    Resolved<String>? find(String field, List<(RunOrigin, RunDefaults?)> layers, {bool fallback = false}) {
      for (final (origin, d) in layers) {
        if (d == null) continue;
        switch (d.mode(field)) {
          case 'value':
            final value = switch (field) {
              'connection' => d.connectionId,
              'model' => d.model,
              _ => d.effort,
            };
            return Resolved(value, origin, fallback: fallback);
          case 'disabled':
            return Resolved(null, origin, fallback: fallback);
        }
      }
      return null;
    }

    Resolved<String> resolve(String field, {bool own = true, bool inherits = true}) =>
        (own ? find(field, mine) : null) ??
        (inherits ? find(field, base, fallback: true) : null) ??
        Resolved(null, RunOrigin.none, fallback: inherits && base.isNotEmpty);

    final subagent = purpose == Purpose.subagent;
    final connectionId = resolve('connection', own: !subagent).value;
    final connection = connections.where((c) => c.id == connectionId).firstOrNull;
    var model = resolve('model', inherits: !subagent);
    if (!subagent && model.value == null && connection?.defaultModel != null) model = Resolved(connection!.defaultModel, RunOrigin.connection, fallback: model.fallback);
    return EmployeeRunView(own: own, connection: connection, model: model, effort: resolve('effort', own: !subagent));
  }
}

/// What the model picker shows for one purpose.
class RunChoice extends Equatable {
  const RunChoice({required this.defaults, required this.connection});

  final RunDefaults defaults;
  final ConnectionView connection;

  /// The chosen model, else the connection's default.
  String? get model => defaults.model ?? connection.defaultModel;
  String? get effort => defaults.effort;

  @override
  List<Object?> get props => [defaults, connection.id];
}

/// Friendly names and descriptions, the same catalogue as the desktop's model popup.
abstract final class ModelCatalog {
  static const _entries = [
    (['opus', 'claude-opus-5-5'], 'Opus 5.5', 'Most capable, for complex work'),
    (['sonnet', 'claude-sonnet-5-5'], 'Sonnet 5.5', 'Smart and fast, for everyday work'),
    (['fable', 'claude-fable-5-1'], 'Fable 5.1', ''),
    (['haiku', 'claude-haiku-4-5', 'claude-haiku-4-5-20251001'], 'Haiku 4.5', 'Fastest, for quick answers'),
  ];

  static const efforts = [
    ('low', 'Low', 'Fastest answers'),
    ('medium', 'Medium', 'Balanced'),
    ('high', 'High', 'Thinks longer'),
    ('xhigh', 'Extra high', 'For hard problems'),
    ('max', 'Max', 'Deepest reasoning, slowest'),
  ];

  static String? describe(String model) {
    for (final (ids, _, detail) in _entries) {
      if (ids.contains(model)) return detail.isEmpty ? null : detail;
    }
    return null;
  }

  static String effortLabel(String? effort) {
    for (final (value, label, _) in efforts) {
      if (value == effort) return label;
    }
    return 'Default';
  }

  /// "claude-opus-5-5" → "Opus 5.5", "gpt-5-mini" → "GPT-5 Mini" (as the desktop).
  static String friendly(String model) {
    final m = model.trim();
    for (final (ids, name, _) in _entries) {
      if (ids.contains(m)) return name;
    }
    if (m.startsWith('fake')) return 'Test runtime';
    final core = m.startsWith('claude-') ? m.substring(7) : m;
    bool digits(String p) => p.isNotEmpty && p.codeUnits.every((c) => c >= 48 && c <= 57);
    bool letters(String p) => p.isNotEmpty && RegExp(r'^[A-Za-z]+$').hasMatch(p);
    final parts = core.split('-').where((p) => digits(p) || letters(p)).toList();
    if (parts.isEmpty) return m;
    String cap(String w) => w[0].toUpperCase() + w.substring(1);
    final first = parts.first;
    var name = first == 'gpt' ? 'GPT' : cap(first);
    final nums = parts.skip(1).where((p) => digits(p) && p.length <= 2).take(2).toList();
    final words = parts.skip(1).where(letters).map(cap);
    if (nums.isNotEmpty) name += (first == 'gpt' ? '-' : ' ') + nums.join('.');
    for (final w in words) {
      name += ' $w';
    }
    return name;
  }
}
