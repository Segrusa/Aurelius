import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/run_config.dart';
import '../../data/views.dart';
import 'my_defaults_bloc.dart';

/// The words of a model choice: the model, or what Default comes to.
String myModelLabel(String? model, {String? fallback}) {
  if (model != null) return ModelCatalog.friendly(model);
  return fallback == null ? 'Default' : 'Default · ${ModelCatalog.friendly(fallback)}';
}

/// Profile > My defaults: the model, effort, mode and subagent model of the
/// tickets this board member makes and the sessions they start.
class MyDefaultsPage extends StatelessWidget {
  const MyDefaultsPage({super.key});

  @override
  Widget build(BuildContext context) {
    return BlocProvider(
      create: (_) => MyDefaultsBloc(context.read<AureliusRepository>())..add(const MyDefaultsRefreshed()),
      child: BlocBuilder<MyDefaultsBloc, MyDefaultsState>(
        builder: (context, state) {
          final view = state.view;
          final bloc = context.read<MyDefaultsBloc>();
          return LargeTitleScaffold(
            title: 'My defaults',
            children: [
              if (view == null && state.error != null)
                EmptyState(
                  'Could not load your defaults.',
                  detail: state.error,
                  action: OutlinedButton(onPressed: () => bloc.add(const MyDefaultsRefreshed()), child: const Text('Try again')),
                )
              else if (view == null)
                const Padding(padding: EdgeInsets.all(32), child: Center(child: CircularProgressIndicator()))
              else ...[
                const SizedBox(height: 20),
                InsetGroup(
                  children: [
                    GroupRow(
                      key: const ValueKey('my-default-model'),
                      title: 'Model',
                      value: myModelLabel(view.defaults.model, fallback: view.model),
                      chevron: true,
                      onTap: state.saving
                          ? null
                          : () => _pick(context, 'Model', view.defaults.model, [
                                (null, myModelLabel(null, fallback: view.defaults.model == null ? view.model : null)),
                                for (final m in state.models) (m, ModelCatalog.friendly(m)),
                              ], MyDefaultsField.model),
                    ),
                    if (view.supportedEfforts.isNotEmpty || view.defaults.effort != null)
                      GroupRow(
                        key: const ValueKey('my-default-effort'),
                        title: 'Effort',
                        value: ModelCatalog.effortLabel(view.defaults.effort),
                        chevron: true,
                        onTap: state.saving
                            ? null
                            : () => _pick(context, 'Effort', view.defaults.effort, [
                                  (null, view.modelDefaultEffort == null ? 'Default' : "Default · ${ModelCatalog.effortLabel(view.modelDefaultEffort)} (model's)"),
                                  for (final e in view.supportedEfforts) (e, ModelCatalog.effortLabel(e)),
                                ], MyDefaultsField.effort),
                      ),
                    GroupRow(
                      key: const ValueKey('my-default-mode'),
                      title: 'Mode',
                      value: view.defaults.mode?.label ?? 'Default · Ask',
                      chevron: true,
                      onTap: state.saving
                          ? null
                          : () => _pick(context, 'Mode', view.defaults.mode?.wire, [
                                (null, 'Default · Ask'),
                                for (final m in agentModes) (m.wire, m.label),
                              ], MyDefaultsField.mode),
                    ),
                    GroupRow(
                      key: const ValueKey('my-default-subagent-model'),
                      title: 'Subagent model',
                      value: view.defaults.subagentModel == null ? 'Default · Sonnet' : ModelCatalog.friendly(view.defaults.subagentModel!),
                      chevron: true,
                      onTap: state.saving
                          ? null
                          : () => _pick(context, 'Subagent model', view.defaults.subagentModel, [
                                (null, 'Default · Sonnet'),
                                for (final m in state.models) (m, ModelCatalog.friendly(m)),
                              ], MyDefaultsField.subagentModel),
                    ),
                  ],
                ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(32, 8, 32, 0),
                  child: Text(
                    'For the tickets you make and the sessions you start. A ticket or a session can still choose for itself. Managers start in Plan, a design in Auto.',
                    style: TextStyle(color: Tokens.muted, fontSize: 13),
                  ),
                ),
                if (state.error != null)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(32, 12, 32, 0),
                    child: Text(state.error!, key: const ValueKey('my-defaults-error'), style: TextStyle(color: Tokens.red, fontSize: 13)),
                  ),
              ],
            ],
          );
        },
      ),
    );
  }

  /// A sheet with [options] (null is Default); a choice goes to the bloc.
  void _pick(BuildContext context, String title, String? current, List<(String?, String)> options, MyDefaultsField field) {
    final bloc = context.read<MyDefaultsBloc>();
    showModalBottomSheet<void>(
      context: context,
      builder: (sheet) => SafeArea(
        child: ListView(
          shrinkWrap: true,
          children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 16, 16, 8), child: Text(title, style: Tokens.title(size: 20))),
            for (final (value, label) in options)
              ListTile(
                key: ValueKey('my-default-option-${value ?? 'default'}'),
                leading: Icon(value == current ? Icons.radio_button_checked : Icons.radio_button_unchecked),
                title: Text(label),
                onTap: () {
                  Navigator.pop(sheet);
                  if (value != current) bloc.add(MyDefaultChosen(field, value));
                },
              ),
          ],
        ),
      ),
    );
  }
}
