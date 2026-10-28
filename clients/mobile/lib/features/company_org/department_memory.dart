import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';

/// What the department remembers (its own memory scope): active items, pinned
/// first. Read-only on the phone; loads once when shown.
class DepartmentMemory extends StatefulWidget {
  const DepartmentMemory({super.key, required this.departmentId});

  final String departmentId;

  @override
  State<DepartmentMemory> createState() => _DepartmentMemoryState();
}

class _DepartmentMemoryState extends State<DepartmentMemory> {
  late final Future<List<MemoryView>> _items = _load();

  Future<List<MemoryView>> _load() async {
    try {
      final items = await context.read<AureliusRepository>().memory(departmentId: widget.departmentId);
      return items.where((m) => m.status == MemoryStatus.active).toList()..sort((a, b) => (b.pinned ? 1 : 0).compareTo(a.pinned ? 1 : 0));
    } catch (_) {
      return const [];
    }
  }

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<List<MemoryView>>(
      future: _items,
      builder: (context, snapshot) {
        final items = snapshot.data ?? const <MemoryView>[];
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            GroupHeader('Memory', count: items.isEmpty ? null : items.length),
            InsetGroup(
              children: [
                if (snapshot.connectionState != ConnectionState.done) const GroupRow(title: 'Loading…', chevron: false),
                if (snapshot.connectionState == ConnectionState.done && items.isEmpty) const GroupRow(title: 'Nothing remembered yet', chevron: false),
                for (final m in items.take(20))
                  Padding(
                    key: ValueKey('department-memory-${m.id}'),
                    padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(m.content, style: const TextStyle(fontSize: 15)),
                        const SizedBox(height: 2),
                        Text(
                          [m.kind.label, if (m.employeeName != null) m.employeeName!, Fmt.ago(m.createdAtMs)].join(' · '),
                          style: TextStyle(fontSize: 12.5, color: Tokens.muted),
                        ),
                      ],
                    ),
                  ),
              ],
            ),
          ],
        );
      },
    );
  }
}
