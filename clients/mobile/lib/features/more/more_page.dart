import 'package:flutter/cupertino.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/widgets/ios.dart';
import '../company/company_bloc.dart';
import '../onboarding/onboarding.dart' show ActiveBoardMemberRow;

/// The More tab: Projects, Tasks (B7: also those without a ticket), Ideas,
/// the company (employees, org chart) and Settings.
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
            GroupRow(
              key: const ValueKey('more-projects'),
              leading: const IconTile(CupertinoIcons.folder_fill),
              title: 'Projects',
              chevron: true,
              onTap: () => context.push('/projects'),
            ),
            GroupRow(
              key: const ValueKey('more-tasks'),
              leading: const IconTile(CupertinoIcons.checkmark_square_fill),
              title: 'Tasks',
              subtitle: 'Also work without a ticket',
              chevron: true,
              onTap: () => context.push('/tasks'),
            ),
            GroupRow(
              key: const ValueKey('more-ideas'),
              leading: const IconTile(CupertinoIcons.lightbulb_fill),
              title: 'Ideas',
              chevron: true,
              onTap: () => context.push('/ideas'),
            ),
            GroupRow(
              key: const ValueKey('more-company'),
              leading: const IconTile(CupertinoIcons.person_3_fill),
              title: 'Company',
              value: employees == 0 ? null : '$employees employee${employees == 1 ? '' : 's'}',
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
