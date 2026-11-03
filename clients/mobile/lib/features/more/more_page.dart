import 'package:flutter/cupertino.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../app/mobile_core.dart';
import '../../core/widgets/ios.dart';
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import '../onboarding/onboarding.dart' show ActiveBoardMemberRow;

/// The More tab: the board member and Settings. Projects, Tasks, Ideas, the Vault, Guidelines
/// and the Company are in the list too, but [kHiddenInMobileCore] leaves them out.
class MorePage extends StatelessWidget {
  const MorePage({super.key});

  @override
  Widget build(BuildContext context) {
    final employees = context.select<CompanyBloc, int>((b) => b.state.activeEmployees.length);
    final vault = context.select<ConnectionBloc, bool>((b) => b.state.has('vault'));
    final guidelines = context.select<ConnectionBloc, bool>((b) => b.state.has('guidelines'));
    return LargeTitleScaffold(
      title: 'More',
      children: [
        const ActiveBoardMemberRow(),
        const SizedBox(height: 20),
        InsetGroup(
          dividerIndent: 60,
          children: [
            if (shownInMobileCore('projects'))
            GroupRow(
              key: const ValueKey('more-projects'),
              leading: const IconTile(CupertinoIcons.folder_fill),
              title: 'Projects',
              chevron: true,
              onTap: () => context.push('/projects'),
            ),
            if (shownInMobileCore('tasks'))
            GroupRow(
              key: const ValueKey('more-tasks'),
              leading: const IconTile(CupertinoIcons.checkmark_square_fill),
              title: 'Tasks',
              subtitle: 'Also work without a ticket',
              chevron: true,
              onTap: () => context.push('/tasks'),
            ),
            if (shownInMobileCore('ideas'))
            GroupRow(
              key: const ValueKey('more-ideas'),
              leading: const IconTile(CupertinoIcons.lightbulb_fill),
              title: 'Ideas',
              chevron: true,
              onTap: () => context.push('/ideas'),
            ),
            if (vault && shownInMobileCore('vault'))
              GroupRow(
                key: const ValueKey('more-vault'),
                leading: const IconTile(CupertinoIcons.lock_fill),
                title: 'Vault',
                subtitle: 'Shared links, notes and secrets',
                chevron: true,
                onTap: () => context.push('/vault'),
              ),
            if (guidelines && shownInMobileCore('guidelines'))
              GroupRow(
                key: const ValueKey('more-guidelines'),
                leading: const IconTile(CupertinoIcons.book_fill),
                title: 'Guidelines',
                subtitle: 'Working rules for employees and projects',
                chevron: true,
                onTap: () => context.push('/guidelines'),
              ),
            if (shownInMobileCore('company'))
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
