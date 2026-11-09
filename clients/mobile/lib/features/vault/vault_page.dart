import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/vault.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import 'vault_audit_page.dart';
import 'vault_bloc.dart';
import 'vault_editor_page.dart';
import 'vault_item_page.dart';

/// The vault (More → Vault, or a project's Vault ›): one screen with the
/// scope on top (the company or a project). A row shows name, key and type,
/// never a secret's value; a tap opens the item with Show and Copy.
class VaultPage extends StatelessWidget {
  const VaultPage({super.key, this.projectId});

  /// The scope to open on; null: the company.
  final String? projectId;

  @override
  Widget build(BuildContext context) {
    if (!context.select<ConnectionBloc, bool>((b) => b.state.has('vault'))) {
      return const LargeTitleScaffold(title: 'Vault', fill: EmptyState('No vault on this server yet', detail: "The server can't do this yet. Update Aurelius on the server."));
    }
    return BlocProvider(
      create: (context) => VaultBloc(context.read<AureliusRepository>(), projectId: projectId),
      child: const _VaultView(),
    );
  }
}

/// The name of a scope: the company's, or the project's (also an archived one).
String scopeName(BuildContext context, String? projectId) {
  if (projectId == null) return 'Company';
  return context.read<CompanyBloc>().state.projects.where((p) => p.id == projectId).firstOrNull?.name ?? 'Project';
}

/// Open [item] with this screen's vault.
Future<void> openVaultItem(BuildContext context, VaultItemView item) {
  final bloc = context.read<VaultBloc>();
  return Navigator.of(context).push(MaterialPageRoute<void>(builder: (_) => BlocProvider.value(value: bloc, child: VaultItemPage(itemId: item.id))));
}

/// The editor for a new item in the shown scope, or for [item].
Future<void> openVaultEditor(BuildContext context, {VaultItemView? item}) {
  final bloc = context.read<VaultBloc>();
  return Navigator.of(context).push(MaterialPageRoute<void>(fullscreenDialog: true, builder: (_) => BlocProvider.value(value: bloc, child: VaultEditorPage(item: item))));
}

class _VaultView extends StatelessWidget {
  const _VaultView();

  Future<void> _chooseScope(BuildContext context) async {
    final bloc = context.read<VaultBloc>();
    final company = context.read<CompanyBloc>().state;
    final counted = {for (final c in bloc.state.list?.counts ?? const <VaultScopeCount>[]) c.projectId};
    // Open projects, and archived ones that still have items (MU4-02 AC3).
    final projects = [
      ...company.visibleProjects,
      ...company.projects.where((p) => p.status == ProjectStatus.archived && counted.contains(p.id)),
    ];
    final picked = await showModalBottomSheet<String>(
      context: context,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ListView(
          shrinkWrap: true,
          children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 16, 16, 4), child: Text('Show the vault of', style: TextStyle(fontSize: 13, color: Tokens.label2))),
            ListTile(
              key: const ValueKey('vault-scope-company'),
              leading: const Icon(CupertinoIcons.house_fill),
              title: const Text('Company'),
              subtitle: Text('${bloc.state.countOf(null)} items'),
              trailing: bloc.state.projectId == null ? const Icon(Icons.check) : null,
              onTap: () => Navigator.pop(sheet, ''),
            ),
            for (final p in projects)
              ListTile(
                key: ValueKey('vault-scope-${p.id}'),
                leading: const Icon(CupertinoIcons.folder),
                title: Text(p.status == ProjectStatus.archived ? '${p.name} (archived)' : p.name),
                subtitle: Text('${bloc.state.countOf(p.id)} items'),
                trailing: bloc.state.projectId == p.id ? const Icon(Icons.check) : null,
                onTap: () => Navigator.pop(sheet, p.id),
              ),
          ],
        ),
      ),
    );
    if (picked == null) return;
    bloc.add(VaultScopeChanged(picked.isEmpty ? null : picked));
  }

  @override
  Widget build(BuildContext context) {
    final state = context.watch<VaultBloc>().state;
    final bloc = context.read<VaultBloc>();
    final scope = scopeName(context, state.projectId);
    return BlocListener<VaultBloc, VaultState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, s) {
        toast(context, s.notice!);
        bloc.add(const VaultNoticeShown());
      },
      child: LargeTitleScaffold(
        title: 'Vault',
        actions: [
          GlassButton(
            tooltip: 'Audit log',
            icon: CupertinoIcons.clock,
            onPressed: () => Navigator.of(context).push(MaterialPageRoute<void>(builder: (_) => VaultAuditLogPage(projectId: state.projectId, scopeName: scope))),
          ),
          GlassButton(tooltip: 'New item', icon: CupertinoIcons.add, onPressed: () => openVaultEditor(context)),
        ],
        onRefresh: () async => bloc.add(const VaultRefreshed()),
        fill: state.list == null
            ? (state.error != null ? ErrorRetry(state.error!, onRetry: () => bloc.add(const VaultRefreshed())) : const Center(child: CircularProgressIndicator.adaptive()))
            : null,
        children: state.list == null
            ? null
            : [
                Padding(
                  padding: const EdgeInsets.fromLTRB(20, 8, 20, 0),
                  child: Align(
                    alignment: Alignment.centerLeft,
                    child: _ScopePill(key: const ValueKey('vault-scope'), label: '$scope · ${state.items.length}', company: state.projectId == null, onTap: () => _chooseScope(context)),
                  ),
                ),
                GroupHeader(state.projectId == null ? 'Company items' : 'Items of $scope', count: state.items.length),
                if (state.items.isEmpty)
                  Padding(
                    key: const ValueKey('vault-empty'),
                    padding: const EdgeInsets.fromLTRB(32, 4, 32, 8),
                    child: Text(
                      state.projectId == null
                          ? 'No items in the company yet. Every employee can read every item here.'
                          : 'No items in $scope yet. Employees of this project can read every item here.',
                      style: TextStyle(fontSize: 15, color: Tokens.label2),
                    ),
                  )
                else
                  InsetGroup(dividerIndent: 60, children: [for (final i in state.items) VaultItemRow(item: i)]),
                if (state.projectId != null) ...[
                  GroupHeader('Company items also available here', count: state.companyItems.length, info: 'A run of this project gets these too. A project item with the same key wins.'),
                  if (state.companyItems.isEmpty)
                    Padding(padding: const EdgeInsets.fromLTRB(32, 4, 32, 8), child: Text('None', style: TextStyle(fontSize: 15, color: Tokens.label2)))
                  else
                    InsetGroup(dividerIndent: 60, children: [for (final i in state.companyItems) VaultItemRow(item: i)]),
                ],
                const VaultNotices(),
              ],
      ),
    );
  }
}

/// The scope on top of the list; a tap chooses another.
class _ScopePill extends StatelessWidget {
  const _ScopePill({super.key, required this.label, required this.company, required this.onTap});

  final String label;
  final bool company;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: 'Scope: $label. Choose another',
        excludeSemantics: true,
        child: GestureDetector(
          onTap: onTap,
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
            decoration: BoxDecoration(color: Tokens.fill, borderRadius: BorderRadius.circular(18)),
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              Icon(company ? CupertinoIcons.house_fill : CupertinoIcons.folder_fill, size: 15, color: Tokens.label2),
              const SizedBox(width: 6),
              Text(label, style: TextStyle(fontSize: 15, fontWeight: FontWeight.w600, color: Tokens.label)),
              const SizedBox(width: 4),
              Icon(CupertinoIcons.chevron_down, size: 13, color: Tokens.label2),
            ]),
          ),
        ),
      );
}

IconData kindIcon(VaultKind kind) => switch (kind) {
      VaultKind.text => CupertinoIcons.text_alignleft,
      VaultKind.url => CupertinoIcons.link,
      VaultKind.secret => CupertinoIcons.lock_fill,
    };

/// One item: name, key and type; a project item that a company item's key
/// also has carries "overrides Company" (B4, P-42). Never a secret's value.
class VaultItemRow extends StatelessWidget {
  const VaultItemRow({super.key, required this.item});

  final VaultItemView item;

  @override
  Widget build(BuildContext context) => Semantics(
        label: item.kind == VaultKind.secret ? hiddenLabel(item) : null,
        child: GroupRow(
          key: ValueKey('vault-item-${item.id}'),
          leading: IconTile(kindIcon(item.kind), color: item.kind == VaultKind.secret ? Tokens.accent : null),
          title: item.name,
          subtitle: '${item.envName} · ${kindLabel(item.kind)}${item.undecryptable == true ? ' · cannot be decrypted' : ''}',
          trailing: overridesCompany(item) ? const OverridesPill() : null,
          chevron: true,
          onTap: () => openVaultItem(context, item),
        ),
      );
}

/// "overrides Company": in runs of the project this item wins (B4).
class OverridesPill extends StatelessWidget {
  const OverridesPill({super.key});

  @override
  Widget build(BuildContext context) => Container(
        key: const ValueKey('overrides-company'),
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: BoxDecoration(color: Tokens.accentTint, borderRadius: BorderRadius.circular(10)),
        child: Text('overrides Company', style: TextStyle(fontSize: 12, fontWeight: FontWeight.w600, color: Tokens.accent)),
      );
}

/// What the vault cannot protect against (E21: R2 and R3), and where the key export is.
class VaultNotices extends StatelessWidget {
  const VaultNotices({super.key});

  @override
  Widget build(BuildContext context) {
    Widget line(Key key, IconData icon, String text) => Padding(
          key: key,
          padding: const EdgeInsets.fromLTRB(32, 6, 32, 0),
          child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Icon(icon, size: 15, color: Tokens.label2),
            const SizedBox(width: 8),
            Expanded(child: Text(text, style: TextStyle(fontSize: 13, color: Tokens.label2))),
          ]),
        );
    return Padding(
      padding: const EdgeInsets.only(top: 18),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        line(const ValueKey('vault-r2'), CupertinoIcons.square_stack_3d_up, vaultMcpNotice),
        line(const ValueKey('vault-r3'), CupertinoIcons.lock_shield, vaultKeyNotice),
        line(const ValueKey('vault-key-export'), CupertinoIcons.desktopcomputer, vaultKeyExportNotice),
      ]),
    );
  }
}
