<?php

declare(strict_types=1);

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

// The planner planning model of the fixture (issue #50): mirrors the
// `planner.user_task_planning` entity with its `planning_id` identity.
// The date/timestamp columns carry no Carbon casts: the maintained
// ports own the canonical wire spelling end to end.

final class UserTaskPlanning extends Model
{
    protected $table = 'user_task_plannings';

    protected $primaryKey = 'planning_id';

    public $incrementing = false;

    protected $keyType = 'string';

    protected $fillable = [
        'planning_id',
        'workspace_id',
        'user_id',
        'task_id',
        'planned_for',
        'position',
        'focused_at',
        'paused_at',
        'completed_at',
        'reorder_version',
    ];
}
