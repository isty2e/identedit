<?php

declare(strict_types=1);

function process_data(int $value): int
{
    return $value + 1;
}

class ExampleService
{
    public function ProcessData(int $value): int
    {
        return process_data($value);
    }
}
