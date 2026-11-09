import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/protocol/models.dart';
import '../../data/run_config.dart';
import 'employee_runs_bloc.dart';

/// One employee's line: the model and the effort a new run uses, and where the
/// effort comes from. Pure, so the words can be tested.
String employeeRunLine(EmployeeRunLine run) {
  final model = run.model == null ? 'Model not known yet' : ModelCatalog.friendly(run.model!);
  final String effort;
  if (run.supportedEfforts.isEmpty) {
    effort = 'No effort setting';
  } else if (run.effort != null) {
    final source = run.effortSource == RunValueSource.employee ? 'chosen for this employee' : 'company default';
    effort = '${ModelCatalog.effortLabel(run.effort)} ($source)';
  } else if (run.modelDefaultEffort != null) {
    effort = "${ModelCatalog.effortLabel(run.modelDefaultEffort)} (model's default)";
  } else {
    effort = "Model's default";
  }
  return '$model · $effort';
}

/// Settings > Model & effort. Changing an effort is done on the desktop.
class EmployeeRunsPage extends StatelessWidget {
  const EmployeeRunsPage({super.key});

  @override
  Widget build(BuildContext context) {
    return BlocProvider(
      create: (_) => EmployeeRunsBloc(context.read<AureliusRepository>())..add(const EmployeeRunsRefreshed()),
      child: BlocBuilder<EmployeeRunsBloc, EmployeeRunsState>(
        builder: (context, state) {
          final view = state.view;
          return LargeTitleScaffold(
            title: 'Model & effort',
            children: [
              if (view == null && state.error != null)
                EmptyState(
                  'Could not load the employees.',
                  detail: state.error,
                  action: OutlinedButton(onPressed: () => context.read<EmployeeRunsBloc>().add(const EmployeeRunsRefreshed()), child: const Text('Try again')),
                )
              else if (view == null)
                const Padding(padding: EdgeInsets.all(32), child: Center(child: CircularProgressIndicator()))
              else ...[
                const SizedBox(height: 20),
                InsetGroup(
                  children: [
                    for (final run in view.employees) GroupRow(title: run.name, subtitle: employeeRunLine(run)),
                  ],
                ),
              ],
            ],
          );
        },
      ),
    );
  }
}
