namespace ExampleApp
{
    public class ExampleService
    {
        private int _offset = 1;

        public int ProcessData(int value)
        {
            return value + _offset;
        }
    }
}
