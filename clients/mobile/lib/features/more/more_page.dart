import 'package:flutter/cupertino.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/widgets/ios.dart';
import '../company/company_bloc.dart';
import '../onboarding/onboarding.dart' show ActiveBoardMemberRow;

/// The More tab: the board member, Employees and Settings.
class MorePage extends StatelessWidget {
  const MorePage({super.key});

  @override
  Widget build(BuildContext context) {
    final employees = context.select<CompanyBloc, int>((b) => b.state.activeEmployees.length);
    return LargeTitleScaffold(
      title: 'More',
      children: [
        const ActiveBoardMemberRow(),
        const SizedBox(height: 20),
        InsetGroup(
          dividerIndent: 60,
          children: [
            // The way to the chat with an employee, and to Give work from an employee's page (Home is gone).
            GroupRow(
              key: const ValueKey('more-employees'),
              leading: const IconTile(CupertinoIcons.person_2_fill),
              title: 'Employees',
              value: employees == 0 ? null : '$employees',
              chevron: true,
              onTap: () => context.push('/company'),
            ),
            GroupRow(
              key: const ValueKey('more-settings'),
              leading: const IconTile(CupertinoIcons.gear_solid),
              title: 'Settings',
              chevron: true,
              onTap: () => context.push('/settings'),
            ),
          ],
        ),
      ],
    );
  }
}
