<?php

declare(strict_types=1);

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

// The planner task model of the fixture: mirrors the `planner.task`
// entity with its `task_id` identity and the observable focused state.

final class Task extends Model
{
    protected $table = 'tasks';

    protected $primaryKey = 'task_id';

    public $incrementing = false;

    protected $keyType = 'string';

    protected $fillable = ['task_id', 'user_id', 'focused', 'focused_at'];

    protected $casts = [
        'focused' => 'boolean',
        // `focused_at` deliberately carries no datetime cast: the port
        // owns the canonical `...Z` spelling end to end, and a Carbon
        // cast would re-serialize it with microseconds, violating the
        // scenario datetime matcher's canonical grammar.
    ];
}
