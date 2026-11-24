import java.io.BufferedReader;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.lang.management.ManagementFactory;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.net.URL;
import java.net.URLClassLoader;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.Properties;
import javax.management.MBeanServer;
import javax.management.ObjectName;

/**
 * Runs TLC jobs one after another in one JVM (#310), so that a job pays TLC's work and not
 * the 0.6 s a JVM takes to start and stop around it.
 *
 * <pre>
 * java -XX:SoftRefLRUPolicyMSPerMB=0 -cp &lt;this class&gt; RdraTlcBatch &lt;tla2tools.jar&gt; [-stats]
 * </pre>
 *
 * It prints {@code ready} once the jar's {@code tlc2.TLC} has the methods a job calls, or
 * {@code unsupported <what is missing>} and stops. Then each line of stdin is one job, its
 * fields separated by tabs:
 *
 * <pre>
 * &lt;id&gt; &lt;file for TLC's stdout&gt; &lt;file for TLC's stderr&gt; &lt;TLC argument&gt;...
 * </pre>
 *
 * A JVM has one working directory, so every path among the arguments is absolute. The answer
 * is one line of stdout: {@code done <id> <exit status> <seconds> <threads left>}. The status
 * is the one a {@code tlc2.TLC} process of these arguments exits with. The threads are those
 * the jobs so far left running, each of which keeps its job's classes and data in the heap;
 * the caller replaces a JVM that has too many. A job that threw is answered
 * {@code failed <id> <why>}. Anything else that ends a job (TLC calling {@code System.exit},
 * the JVM out of memory) ends the JVM without an answer, and the caller runs that job as a
 * process of its own. With {@code -stats}, {@code done} also carries the heap in use after a
 * GC and the classes loaded.
 *
 * <p>Each job loads TLC through a class loader of its own, because TLC keeps a run's state in
 * static fields, and runs in a thread and thread group of its own, so that what it leaves in
 * thread-local variables goes with it and the threads it leaves can be counted.
 * {@link #leave} undoes what a run registers with the JVM. The RMI runtime keeps the classes
 * of every exported object behind a soft reference, and TLC 1.7.4 exports its fingerprint
 * set, so the JVM has to clear soft references at each GC
 * ({@code -XX:SoftRefLRUPolicyMSPerMB=0}) for a job's classes to be unloaded; TLC itself has
 * no soft reference. Written for Java 8, the oldest TLC 1.7.4 runs on.
 */
public final class RdraTlcBatch {
    private final URL jar;
    private final PrintStream out;
    private final PrintStream err;
    private final Properties properties;
    /** The thread groups of the jobs so far that still have a thread. */
    private final List<ThreadGroup> left = new ArrayList<ThreadGroup>();

    private RdraTlcBatch(URL jar) {
        this.jar = jar;
        this.out = System.out;
        this.err = System.err;
        this.properties = (Properties) System.getProperties().clone();
    }

    public static void main(String[] argv) throws Exception {
        if (argv.length < 1) {
            System.err.println("usage: RdraTlcBatch <tla2tools.jar> [-stats]");
            System.exit(2);
        }
        boolean stats = argv.length > 1 && argv[1].equals("-stats");
        RdraTlcBatch batch = new RdraTlcBatch(new File(argv[0]).toURI().toURL());
        String missing = batch.missing();
        if (missing != null) {
            batch.out.println("unsupported " + missing);
            batch.out.flush();
            System.exit(3);
        }
        batch.out.println("ready");
        batch.out.flush();
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in, "UTF-8"));
        for (String line = in.readLine(); line != null; line = in.readLine()) {
            if (line.isEmpty()) {
                continue;
            }
            String[] job = line.split("\t", -1);
            String answer;
            if (job.length < 3) {
                answer = "failed " + job[0] + " a job is <id> <stdout> <stderr> <arg>...";
            } else {
                long start = System.nanoTime();
                try {
                    int status = batch.run(job[0], job[1], job[2], Arrays.copyOfRange(job, 3, job.length));
                    double seconds = (System.nanoTime() - start) / 1e9;
                    answer = String.format(Locale.ROOT, "done %s %d %.3f %d", job[0], status, seconds, batch.threadsLeft());
                    if (stats) {
                        answer += stats();
                    }
                } catch (Throwable e) {
                    answer = "failed " + job[0] + " " + String.valueOf(e).replace('\n', ' ');
                }
            }
            batch.out.println(answer);
            batch.out.flush();
        }
        // The threads jobs left are not daemons.
        System.exit(0);
    }

    private URLClassLoader load() {
        return new URLClassLoader(new URL[] {jar}, ClassLoader.getSystemClassLoader().getParent());
    }

    /** What a job needs of the jar and the jar does not have, or null. */
    private String missing() {
        try (URLClassLoader loader = load()) {
            Class<?> tlc = loader.loadClass("tlc2.TLC");
            tlc.getConstructor();
            tlc.getMethod("handleParameters", String[].class);
            tlc.getMethod("setResolver", loader.loadClass("util.FilenameToStream"));
            Method process = tlc.getMethod("process");
            if (process.getReturnType() != int.class) {
                return "tlc2.TLC.process() returns " + process.getReturnType().getName();
            }
            tlc.getMethod("getMainFile");
            loader.loadClass("util.SimpleFilenameToStream").getConstructor(String.class);
            loader.loadClass("util.FileUtil").getMethod("parseDirname", String.class);
            loader.loadClass("tlc2.output.EC$ExitStatus").getMethod("errorConstantToExitStatus", int.class);
            return null;
        } catch (Throwable e) {
            return String.valueOf(e).replace('\n', ' ');
        }
    }

    /** One job, in a thread and a thread group of its own. */
    private int run(String id, final String stdout, final String stderr, final String[] args) throws Throwable {
        final Object[] result = new Object[1];
        ThreadGroup group = new ThreadGroup("tlc-job-" + id);
        Thread job = new Thread(group, "tlc-job-" + id) {
            @Override
            public void run() {
                try {
                    result[0] = inThread(stdout, stderr, args);
                } catch (Throwable e) {
                    result[0] = e;
                }
            }
        };
        job.start();
        job.join();
        left.add(group);
        if (result[0] instanceof Throwable) {
            throw (Throwable) result[0];
        }
        return (Integer) result[0];
    }

    /** The threads the jobs so far left running. A TLC worker may take a moment to end. */
    private int threadsLeft() {
        int threads = 0;
        for (Iterator<ThreadGroup> groups = left.iterator(); groups.hasNext(); ) {
            int active = groups.next().activeCount();
            if (active == 0) {
                groups.remove();
            }
            threads += active;
        }
        return threads;
    }

    private int inThread(String stdout, String stderr, String[] args) throws Throwable {
        URLClassLoader loader = load();
        try (PrintStream jobOut = new PrintStream(new FileOutputStream(stdout), true);
                PrintStream jobErr = new PrintStream(new FileOutputStream(stderr), true)) {
            // TLC reads System.out once, when its classes load.
            System.setOut(jobOut);
            System.setErr(jobErr);
            Thread.currentThread().setContextClassLoader(loader);
            try {
                return tlc(loader, args);
            } catch (InvocationTargetException e) {
                throw e.getCause() == null ? e : e.getCause();
            } finally {
                leave(loader);
            }
        } finally {
            System.setOut(out);
            System.setErr(err);
            System.setProperties((Properties) properties.clone());
            loader.close();
        }
    }

    /** What {@code tlc2.TLC.main} does with `args`, returning the status it would exit with. */
    private static int tlc(ClassLoader loader, String[] args) throws Exception {
        Class<?> tlc = loader.loadClass("tlc2.TLC");
        Object run = tlc.getConstructor().newInstance();
        if (!Boolean.TRUE.equals(tlc.getMethod("handleParameters", String[].class).invoke(run, (Object) args))) {
            return 1;
        }
        // Private in 1.7.4: it warns when the collector is not the parallel one.
        try {
            Method check = tlc.getDeclaredMethod("checkEnvironment");
            check.setAccessible(true);
            if (Boolean.FALSE.equals(check.invoke(run))) {
                return 1;
            }
        } catch (NoSuchMethodException e) {
            // A TLC without the check has no warning to print.
        }
        String main = (String) tlc.getMethod("getMainFile").invoke(run);
        String dir = (String) loader.loadClass("util.FileUtil").getMethod("parseDirname", String.class).invoke(null, main);
        Class<?> simple = loader.loadClass("util.SimpleFilenameToStream");
        Object resolver = dir.isEmpty()
                ? simple.getConstructor().newInstance()
                : simple.getConstructor(String.class).newInstance(dir);
        tlc.getMethod("setResolver", loader.loadClass("util.FilenameToStream")).invoke(run, resolver);
        int result = (Integer) tlc.getMethod("process").invoke(run);
        return (Integer) loader.loadClass("tlc2.output.EC$ExitStatus")
                .getMethod("errorConstantToExitStatus", int.class)
                .invoke(null, result);
    }

    /**
     * Undoes what a TLC run registers with the JVM and a process would take with it: the
     * MBeans, which the next run would fail to register under the same names, and the RMI
     * export of the fingerprint set.
     */
    private static void leave(ClassLoader loader) {
        try {
            MBeanServer server = ManagementFactory.getPlatformMBeanServer();
            for (ObjectName name : server.queryNames(new ObjectName("tlc2.*:*"), null)) {
                server.unregisterMBean(name);
            }
        } catch (Exception e) {
            // The next run reports a name it cannot register.
        }
        try {
            Object checker = loader.loadClass("tlc2.TLCGlobals").getField("mainChecker").get(null);
            if (checker != null) {
                Object fingerprints = checker.getClass().getField("theFPSet").get(checker);
                if (fingerprints != null) {
                    fingerprints.getClass().getMethod("unexportObject", boolean.class).invoke(fingerprints, true);
                }
            }
        } catch (Throwable e) {
            // A run that built no fingerprint set exported none.
        }
    }

    private static String stats() {
        System.gc();
        Runtime runtime = Runtime.getRuntime();
        return String.format(
                Locale.ROOT,
                " heap=%d classes=%d",
                runtime.totalMemory() - runtime.freeMemory(),
                ManagementFactory.getClassLoadingMXBean().getLoadedClassCount());
    }
}
