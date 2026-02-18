public class DuplicateMethods {
    static class Alpha {
        private void configure() {
            System.out.println("ready");
        }
    }

    static class Beta {
        private void configure() {
            System.out.println("ready");
        }
    }
}
