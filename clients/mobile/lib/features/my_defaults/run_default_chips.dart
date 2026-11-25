import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../data/aurelius_repository.dart';
import '../runtime/runtime_bloc.dart';
import 'my_defaults_bloc.dart' show userRunDefaultsFeature;
import 'run_default_label.dart';

/// What the Model, Effort and Mode chips say without a choice for work on
/// [loginId]'s login ([loginName]; none: the reader's own). Null on a server
/// without `user_run_defaults`: the caller keeps its old label there.
RunDefaultLabels? watchRunDefaultLabels(BuildContext context, {String? loginId, String? loginName}) =>
    _labels(context, context.watch<RuntimeBloc>(), loginId, loginName);

/// [watchRunDefaultLabels] once, outside a build (a sheet about to open).
RunDefaultLabels? readRunDefaultLabels(BuildContext context, {String? loginId, String? loginName}) =>
    _labels(context, context.read<RuntimeBloc>(), loginId, loginName);

RunDefaultLabels? _labels(BuildContext context, RuntimeBloc runtime, String? loginId, String? loginName) {
  final link = context.read<AureliusRepository>().linkState;
  if (!link.has(userRunDefaultsFeature)) return null;
  final meId = link.user?.id;
  final other = loginId != null && meId != null && loginId != meId;
  if (other && !runtime.state.others.containsKey(loginId)) runtime.add(RuntimeLoginWanted(loginId));
  return RunDefaultLabels.of(whoseDefault(loginId: loginId, loginName: loginName, meId: meId), runtime.state.defaultsOf(loginId, meId));
}

/// The last row of a run sheet: My defaults is the only place to change them.
class ChangeMyDefaultsTile extends StatelessWidget {
  const ChangeMyDefaultsTile({super.key, required this.sheet});

  /// The sheet's own context, closed before My defaults opens.
  final BuildContext sheet;

  @override
  Widget build(BuildContext context) {
    final router = GoRouter.of(context);
    return ListTile(
      key: const Key('change-my-defaults'),
      leading: Icon(Icons.tune, color: Tokens.accentText),
      title: Text('Change my defaults', style: TextStyle(color: Tokens.accentText)),
      onTap: () {
        Navigator.pop(sheet);
        router.push(myDefaultsRoute);
      },
    );
  }
}
