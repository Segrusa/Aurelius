import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/company/project_order.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'support/builders.dart';

class _Repo extends Mock implements AureliusRepository {}

void main() {
  final a = projectOf(id: 'a', name: 'Atlas');
  final b = projectOf(id: 'b', name: 'Borealis');
  final c = projectOf(id: 'c', name: 'Cygnus');

  test('the saved order wins; projects it does not know follow in server order', () {
    expect(inProjectOrder([a, b, c], const []).map((p) => p.id), ['a', 'b', 'c']);
    expect(inProjectOrder([a, b, c], const ['c', 'gone', 'a']).map((p) => p.id), ['c', 'a', 'b']);
  });

  test('a drag moves one row to where it ends', () {
    expect(movedProject([a, b, c], 0, 2), ['b', 'c', 'a']);
    expect(movedProject([a, b, c], 2, 0), ['c', 'a', 'b']);
    expect(movedProject([a, b, c], 1, 1), ['a', 'b', 'c']);
  });

  test('the store keeps one order per host', () async {
    SharedPreferences.setMockInitialValues({});
    await const ProjectOrderStore('h1').save(['b', 'a']);
    expect(await const ProjectOrderStore('h1').load(), ['b', 'a']);
    expect(await const ProjectOrderStore('h2').load(), isEmpty);
  });

  test('the bloc reorders the visible projects, keeps archived ones and saves', () async {
    SharedPreferences.setMockInitialValues({});
    final archived = projectOf(id: 'x', name: 'Old', status: ProjectStatus.archived);
    await const ProjectOrderStore('h').save(['x', 'a', 'b', 'c']);
    final repo = _Repo();
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
    when(() => repo.changes).thenAnswer((_) => const Stream.empty());
    when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.online));
    when(() => repo.projects()).thenAnswer((_) async => [a, b, c, archived]);
    when(() => repo.company()).thenAnswer((_) async => companyOf());
    when(() => repo.proposals()).thenAnswer((_) async => const []);
    when(() => repo.tasks()).thenAnswer((_) async => const []);

    final bloc = CompanyBloc(repo, order: const ProjectOrderStore('h'));
    await Future<void>.delayed(const Duration(milliseconds: 400));
    expect(bloc.state.visibleProjects.map((p) => p.id), ['a', 'b', 'c']);

    bloc.add(const ProjectsReordered(2, 0));
    await Future<void>.delayed(const Duration(milliseconds: 50));
    expect(bloc.state.visibleProjects.map((p) => p.id), ['c', 'a', 'b']);
    expect(await const ProjectOrderStore('h').load(), ['c', 'a', 'b', 'x']);
    await bloc.close();
  });
}
