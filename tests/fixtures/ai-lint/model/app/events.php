<?php
// Synthetic UTF-8: caf? ??.
use Illuminate\Database\Eloquent\Model;
class Task extends Model { public string $state = 'backlog'; }
class TaskObserver { public function saved(Task $task) { $task->state = 'done'; } }
Task::observe(TaskObserver::class);
class Commands {
  public function focus(Task $task, $method) { $task->save(); $task->$method(); }
  public function edit(Task $task) { $task->save(); }
  public function plain(Task $task) { $task->state = 'focused'; }
}
$text = 'magic-reflection-observer';
