import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/commands.dart';
import '../../data/company.dart' as org;
import '../../data/views.dart';
import 'project_order.dart';

sealed class CompanyEvent {
  const CompanyEvent();
}

class CompanyRefreshed extends CompanyEvent {
  const CompanyRefreshed();
}

class ProjectCreated extends CompanyEvent {
  const ProjectCreated(this.name, {this.description});

  final String name;
  final String? description;
}

class _OrderLoaded extends CompanyEvent {
  const _OrderLoaded(this.ids);

  final List<String> ids;
}

class CompanyNoticeShown extends CompanyEvent {
  const CompanyNoticeShown();
}

/// "Run tasks one at a time" for one employee; sent only when it changes.
class EmployeeSequentialToggled extends CompanyEvent {
  const EmployeeSequentialToggled(this.employeeId, this.value);

  final String employeeId;
  final bool value;
}

/// Any employee or company command from the company pages
/// (employee pages, org chart, Home's Give work).
class CompanyCommandSent extends CompanyEvent {
  const CompanyCommandSent(this.command, {this.done, this.expectedVersion, this.retry = true, this.then = const []});

  final Map<String, dynamic> command;

  /// Sent in order once [command] committed (an edit that also moves the employee in the org chart).
  final List<Map<String, dynamic>> then;

  /// Shown after it committed.
  final String? done;
  final int? expectedVersion;

  /// Decisions (archive, decide a proposal) are never replayed later.
  final bool retry;
}

/// The user dragged a project row from [from] so it ends at [to] (indices
/// into [CompanyState.visibleProjects]).
class ProjectsReordered extends CompanyEvent {
  const ProjectsReordered(this.from, this.to);

  final int from;
  final int to;
}

class CompanyState extends Equatable {
  const CompanyState({
    this.loading = true,
    this.projects = const [],
    this.employees = const [],
    this.proposals = const [],
    this.tasks = const [],
    this.followOrgChart = true,
    this.company,
    this.notice,
    this.createdProjectId,
    this.lastResult,
    this.order = const [],
  });

  final bool loading;
  final List<ProjectSummary> projects;

  /// The user's order of the projects (ids), saved on this phone.
  final List<String> order;

  /// Everyone in the company, archived ones included.
  final List<EmployeeSummary> employees;

  /// Colleague and instruction proposals of employees, pending and decided.
  final List<ProposalView> proposals;

  /// All tasks of all projects.
  final List<TaskView> tasks;

  /// The server setting "Follow the org chart".
  final bool followOrgChart;

  /// The company itself: its name and default dispatcher (null until loaded).
  final CompanyView? company;
  final String? notice;

  /// Set once after "New project" so the UI can open it.
  final String? createdProjectId;

  /// The last command result (a task given from Home → open it).
  final CommandResult? lastResult;

  List<ProjectSummary> get visibleProjects => inProjectOrder(projects.where((p) => p.status != ProjectStatus.archived).toList(), order);

  /// Projects work can be given in (not archived), in the user's order.
  List<ProjectSummary> get openProjects => visibleProjects;

  List<EmployeeSummary> get activeEmployees => org.activeEmployees(employees);

  /// Root employees: the management team the Board gives work to.
  List<EmployeeSummary> get managementTeam => org.managementTeam(employees);

  List<ProposalView> get pendingProposals => proposals.where((p) => p.isPending).toList();

  /// Employees working right now (archived ones left out).
  List<EmployeeSummary> get workingEmployees => employees.where((m) => m.working && m.status != LifecycleStatus.archived).toList();

  /// What an employee is busy with: its most recently touched running task, if any.
  TaskView? currentTask(String employeeId) {
    final running = tasks.where((t) => t.employeeId == employeeId && (t.status == TaskStatus.running || t.status == TaskStatus.review)).toList()
      ..sort((a, b) => b.updatedAtMs.compareTo(a.updatedAtMs));
    return running.firstOrNull;
  }

  int get working => workingEmployees.length;
  int get waiting => tasks.where((t) => t.needsYou).length;

  /// Who works in a project: employees with open tasks there.
  List<EmployeeSummary> employeesIn(String projectId) => org.workingIn(projectId, activeEmployees, tasks);
  int waitingIn(String projectId) => tasks.where((t) => t.projectId == projectId && t.needsYou).length;
  ProjectSummary? project(String id) => projects.where((p) => p.id == id).firstOrNull;
  EmployeeSummary? employee(String id) => employees.where((m) => m.id == id).firstOrNull;

  CompanyState copyWith({
    bool? loading,
    List<ProjectSummary>? projects,
    List<EmployeeSummary>? employees,
    List<ProposalView>? proposals,
    List<TaskView>? tasks,
    bool? followOrgChart,
    CompanyView? company,
    String? notice,
    String? createdProjectId,
    CommandResult? lastResult,
    List<String>? order,
  }) =>
      CompanyState(
        loading: loading ?? this.loading,
        projects: projects ?? this.projects,
        employees: employees ?? this.employees,
        proposals: proposals ?? this.proposals,
        tasks: tasks ?? this.tasks,
        followOrgChart: followOrgChart ?? this.followOrgChart,
        company: company ?? this.company,
        notice: notice,
        createdProjectId: createdProjectId,
        lastResult: lastResult,
        order: order ?? this.order,
      );

  @override
  List<Object?> get props => [loading, projects, employees, proposals, tasks, followOrgChart, company?.dispatcherEmployeeId, company?.testerEmployeeId, company?.version, notice, createdProjectId, lastResult, order];
}

/// The company: employees, projects and their tasks, what waits
/// for the user, and the commands of the company pages.
class CompanyBloc extends Bloc<CompanyEvent, CompanyState> {
  CompanyBloc(this._repository, {ProjectOrderStore? order}) : _order = order, super(const CompanyState()) {
    on<CompanyRefreshed>(_refresh, transformer: debounceRestartable());
    on<ProjectCreated>(_create);
    on<CompanyNoticeShown>((e, emit) => emit(state.copyWith()));
    on<EmployeeSequentialToggled>(_sequential);
    on<CompanyCommandSent>(_command);
    on<ProjectsReordered>(_reorder);
    on<_OrderLoaded>((e, emit) => emit(state.copyWith(order: e.ids)));
    order?.load().then((ids) {
      if (!isClosed && ids.isNotEmpty) add(_OrderLoaded(ids));
    });
    _subs = [
      _repository.online.listen((_) => add(const CompanyRefreshed())),
      // Presence derives from runs, so chat events matter here too.
      _repository.changes.listen((_) => add(const CompanyRefreshed())),
    ];
    if (_repository.linkState.isOnline) add(const CompanyRefreshed());
  }

  final AureliusRepository _repository;
  final ProjectOrderStore? _order;
  late final List<StreamSubscription<void>> _subs;

  void _reorder(ProjectsReordered event, Emitter<CompanyState> emit) {
    final visible = state.visibleProjects;
    final ids = movedProject(visible, event.from, event.to);
    // Archived projects keep their place at the end, should they come back.
    final rest = state.order.where((id) => !ids.contains(id));
    final order = [...ids, ...rest];
    emit(state.copyWith(order: order));
    _order?.save(order);
  }

  Future<void> _refresh(CompanyRefreshed event, Emitter<CompanyState> emit) async {
    try {
      final projects = await _repository.projects();
      final company = await _repository.company();
      final tasks = await _repository.tasks();
      final proposals = await _repository.proposals();
      emit(state.copyWith(
        loading: false,
        projects: projects,
        employees: company.employees,
        followOrgChart: company.followOrgChart,
        company: company.company,
        // Proposed instructions only where the server has the feature.
        proposals: proposals.where((p) => !p.isInstructions || _repository.linkState.has(Harness2.instructionProposals)).toList(),
        tasks: tasks,
      ));
    } on DisconnectedException {
      emit(state.copyWith(loading: false));
    } on ProtocolException catch (e) {
      emit(state.copyWith(loading: false, notice: e.message));
    }
  }

  Future<void> _create(ProjectCreated event, Emitter<CompanyState> emit) async {
    try {
      final receipt = await _repository.write(Commands.createProject(event.name, description: event.description));
      add(const CompanyRefreshed());
      emit(state.copyWith(createdProjectId: switch (receipt.command) { CommandResultProjectCreated(:final projectId) => projectId, _ => null }));
    } on NotSentException catch (e) {
      emit(state.copyWith(notice: e.toString()));
    } on ProtocolException catch (e) {
      emit(state.copyWith(notice: e.message));
    }
  }

  Future<void> _command(CompanyCommandSent event, Emitter<CompanyState> emit) async {
    try {
      final receipt = await _repository.write(event.command, expectedVersion: event.expectedVersion, retry: event.retry);
      for (final next in event.then) {
        await _repository.write(next, retry: event.retry);
      }
      emit(state.copyWith(notice: event.done, lastResult: receipt.command));
      add(const CompanyRefreshed());
    } on NotSentException catch (e) {
      emit(state.copyWith(notice: e.toString()));
    } on ProtocolException catch (e) {
      emit(state.copyWith(notice: e.isConflict ? 'Someone changed this in the meantime. Showing the latest version.' : e.message));
      add(const CompanyRefreshed());
    }
  }

  void send(Map<String, dynamic> command, {String? done, int? expectedVersion, bool retry = true, List<Map<String, dynamic>> then = const []}) =>
      add(CompanyCommandSent(command, done: done, expectedVersion: expectedVersion, retry: retry, then: then));

  Future<void> _sequential(EmployeeSequentialToggled event, Emitter<CompanyState> emit) async {
    final m = state.employee(event.employeeId);
    if (m == null || m.sequentialTasks == event.value) return;
    List<EmployeeSummary> toggled(bool value) => [for (final x in state.employees) x.id == m.id ? x.withSequentialTasks(value) : x];
    emit(state.copyWith(employees: toggled(event.value)));
    try {
      await _repository.write(
        Commands.updateEmployee(m.id, m.name, m.role, instructions: m.instructions, sequentialTasks: event.value),
        expectedVersion: m.version,
        retry: false,
      );
      add(const CompanyRefreshed());
    } on NotSentException catch (e) {
      emit(state.copyWith(employees: toggled(m.sequentialTasks), notice: e.toString()));
    } on ProtocolException catch (e) {
      emit(state.copyWith(employees: toggled(m.sequentialTasks), notice: e.message));
    }
  }

  @override
  Future<void> close() async {
    for (final s in _subs) {
      await s.cancel();
    }
    return super.close();
  }
}
