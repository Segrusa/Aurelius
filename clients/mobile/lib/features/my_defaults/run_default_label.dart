// One label for "no choice for this run" (OPR-5 point 5), the same words as the
// desktop: the board member whose login the work runs on decides the model,
// effort and mode, so a chip says whose default it is and what it comes to:
// "Your default · Opus 5.5", "Kim's default · High".

import '../../data/run_config.dart' show ModelCatalog;
import '../../data/views.dart';

/// The server reads another board member's run defaults (`user.run_defaults_of`).
const runDefaultsOfFeature = 'run_defaults_of';

/// Where the chips' "Change my defaults" leads.
const myDefaultsRoute = '/settings/my-defaults';

/// "Your default", or "Kim's default" when the work runs on another board
/// member's login ([loginId], [loginName]).
String whoseDefault({String? loginId, String? loginName, String? meId}) =>
    loginId != null && loginName != null && meId != null && loginId != meId ? "$loginName's default" : 'Your default';

/// "Your default · Opus 5.5", or just "Your default" while the value is unknown.
String defaultLabel(String whose, String? value) => value == null || value.isEmpty ? whose : '$whose · $value';

/// The first entry of a chip's sheet: "Use your default", "Use Kim's default".
String resetLabel(String whose) => whose == 'Your default' ? 'Use your default' : 'Use $whose';

/// The model a run without a choice uses: the chosen one, else the connection's.
String? defaultModelOf(UserRunDefaultsView? view) => view?.model;

/// The effort a run without a choice uses: the chosen one, else the model's own.
String? defaultEffortOf(UserRunDefaultsView? view) => view?.defaults.effort ?? view?.modelDefaultEffort;

/// The mode a new conversation starts in without a choice: the chosen one, else
/// Ask (a manager still starts in Plan; the server decides that).
AgentMode defaultModeOf(UserRunDefaultsView? view) => view?.defaults.mode ?? AgentMode.ask;

/// What the Model, Effort and Mode chips say without a choice for this run.
class RunDefaultLabels {
  const RunDefaultLabels({required this.whose, this.modelValue, this.effortValue, this.modeValue});

  /// What the chips say for [view], the defaults of the board member [whose]
  /// names; a null [view] (not read yet, or an older server) gives only whose.
  factory RunDefaultLabels.of(String whose, UserRunDefaultsView? view) {
    final model = defaultModelOf(view);
    final effort = defaultEffortOf(view);
    return RunDefaultLabels(
      whose: whose,
      modelValue: model == null ? null : ModelCatalog.friendly(model),
      effortValue: effort == null ? null : ModelCatalog.effortLabel(effort),
      modeValue: view == null ? null : defaultModeOf(view).label,
    );
  }

  /// "Your default", "Kim's default".
  final String whose;

  /// What the default comes to by name ("Opus 5.5", "High", "Ask"); null while unknown.
  final String? modelValue;
  final String? effortValue;
  final String? modeValue;

  /// "Your default · Opus 5.5".
  String get model => defaultLabel(whose, modelValue);

  /// "Your default · High".
  String get effort => defaultLabel(whose, effortValue);

  /// "Your default · Ask".
  String get mode => defaultLabel(whose, modeValue);

  /// "Use your default".
  String get reset => resetLabel(whose);
}
