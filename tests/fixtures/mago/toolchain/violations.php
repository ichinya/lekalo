<?php

namespace Fixture;

class Violations
{
    public function add($left, $right)
    {
        $name = 'amount';
        $$name = 5;

        return $$name + $left;
    }
}
