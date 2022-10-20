use std::marker::PhantomData;

/// Lock level of a mutex
///
/// While a mutex of L1 is locked on a thread, only mutexes of L2 or higher may be locked.
/// This lock hierarchy prevents deadlocks from occurring. For a dead lock to occour
/// We need some thread TA to hold a resource RA, and request a resource RB, while
/// another thread TB holds RB, and requests RA. This is not possible with a lock
/// hierarchy either RA or RB must be on a level that the other.
///
/// At some point in time we would want Level to be replaced by usize, however
/// with current cont generics (rust 1.55), we cannot compare const generic arguments
/// so we are left with this mess.
pub trait Level {}

/// Indicate that the implementor is lower that the level O
pub trait Lower<O: Level>: Level {}

/// Lowest locking level, no locks can be on this level
#[derive(Debug)]
pub struct L0 {}

#[derive(Debug)]
pub struct L1 {}

#[derive(Debug)]
pub struct L2 {}

#[derive(Debug)]
pub struct L3 {}

#[derive(Debug)]
pub struct L4 {}

#[derive(Debug)]
pub struct L5 {}

impl Level for L0 {}
impl Level for L1 {}
impl Level for L2 {}
impl Level for L3 {}
impl Level for L4 {}
impl Level for L5 {}

impl Lower<L1> for L0 {}
impl Lower<L2> for L0 {}
impl Lower<L3> for L0 {}
impl Lower<L4> for L0 {}
impl Lower<L5> for L0 {}

impl Lower<L2> for L1 {}
impl Lower<L3> for L1 {}
impl Lower<L4> for L1 {}
impl Lower<L5> for L1 {}

impl Lower<L3> for L2 {}
impl Lower<L4> for L2 {}
impl Lower<L5> for L2 {}

impl Lower<L4> for L3 {}
impl Lower<L5> for L3 {}

impl Lower<L5> for L4 {}

/// Indicate that the implementor is higher that the level O
pub trait Higher<O: Level>: Level {}
impl<L1: Level, L2: Level> Higher<L2> for L1 where L2: Lower<L1> {}

/// While this exists only locks with a level higher than L, may be locked.
/// These tokens are carried around the call stack to indicate tho current locking level.
/// They have no size and should disappear at runtime.
pub struct LockToken<'a, L: Level>(PhantomData<&'a mut L>);

impl<'a, L: Level> LockToken<'a, L> {
    /// Create a borrowed copy of self
    pub fn token(&mut self) -> LockToken<'_, L> {
        LockToken(Default::default())
    }

    /// Create a borrowed copy of self, on a higher level
    pub fn downgrade<LC: Higher<L>>(&mut self) -> LockToken<'_, LC> {
        LockToken(Default::default())
    }

    pub fn downgraded<LP: Lower<L>>(_: LockToken<'a, LP>) -> Self {
        LockToken(Default::default())
    }
}

/// Token indicating that there are no acquired locks while not borrowed.
pub struct CleanLockToken;

impl CleanLockToken {
    /// Create a borrowed copy of self
    pub fn token(&mut self) -> LockToken<'_, L0> {
        LockToken(Default::default())
    }

    /// Create a borrowed copy of self, on a higher level
    pub fn downgrade<L: Level>(&mut self) -> LockToken<'_, L> {
        LockToken(Default::default())
    }

    /// Create a new instance
    ///
    /// # Safety
    ///
    /// This is safe to call as long as there are no currently acquired locks
    /// in the thread/task, and as long as there are no other CleanLockToken
    /// in the thread/task.
    ///
    /// A CleanLockToken
    pub unsafe fn new() -> Self {
        CleanLockToken
    }
}

/// A mutual exclusion primitive useful for protecting shared data
///
/// This mutex will block threads waiting for the lock to become available. The
/// mutex can also be statically initialized or created via a `new`
/// constructor. Each mutex has a type parameter which represents the data that
/// it is protecting. The data can only be accessed through the RAII guards
/// returned from `lock` and `try_lock`, which guarantees that the data is only
/// ever accessed when the mutex is locked.
#[derive(Debug)]
pub struct Mutex<L: Level, T> {
    inner: std::sync::Mutex<T>,
    _phantom: PhantomData<L>,
}

impl<L: Level, T: Default> Default for Mutex<L, T> {
    fn default() -> Self {
        Self {
            inner: Default::default(),
            _phantom: Default::default(),
        }
    }
}

impl<L: Level, T> Mutex<L, T> {
    /// Creates a new mutex in an unlocked state ready for use
    pub fn new(val: T) -> Self {
        Self {
            inner: std::sync::Mutex::new(val),
            _phantom: Default::default(),
        }
    }

    /// Acquires a mutex, blocking the current thread until it is able to do so.
    ///
    /// This function will block the local thread until it is available to acquire the mutex.
    /// Upon returning, the thread is the only thread with the mutex held.
    /// An RAII guard is returned to allow scoped unlock of the lock. When the guard goes out of scope, the mutex will be unlocked.
    pub fn lock<'a, LP: Lower<L> + 'a>(
        &'a self,
        lock_token: LockToken<'a, LP>,
    ) -> MutexGuard<'a, L, T> {
        MutexGuard {
            inner: self.inner.lock().unwrap(),
            lock_token: LockToken::downgraded(lock_token),
        }
    }

    /// Attempts to acquire this lock.
    ///
    /// If the lock could not be acquired at this time, then `None` is returned.
    /// Otherwise, an RAII guard is returned. The lock will be unlocked when the
    /// guard is dropped.
    ///
    /// This function does not block.
    pub fn try_lock<'a, LP: Lower<L> + 'a>(
        &'a self,
        lock_token: LockToken<'a, LP>,
    ) -> Option<MutexGuard<'a, L, T>> {
        match self.inner.try_lock() {
            Ok(inner) => Some(MutexGuard {
                inner,
                lock_token: LockToken::downgraded(lock_token),
            }),
            Err(std::sync::TryLockError::Poisoned(_)) => panic!("Poised lock"),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }

    /// Consumes this Mutex, returning the underlying data.
    pub fn into_inner(self) -> T {
        self.inner.into_inner().unwrap()
    }
}

/// An RAII implementation of a "scoped lock" of a mutex. When this structure is
/// dropped (falls out of scope), the lock will be unlocked.
///
/// The data protected by the mutex can be accessed through this guard via its
/// `Deref` and `DerefMut` implementations.
pub struct MutexGuard<'a, L: Level, T: ?Sized + 'a> {
    inner: std::sync::MutexGuard<'a, T>,
    lock_token: LockToken<'a, L>,
}

impl<'a, L: Level, T: ?Sized + 'a> MutexGuard<'a, L, T> {
    /// Split the guard into two parts, the first a mutable reference to the held content
    /// the second a [`LockToken`] that can be used for further locking
    pub fn token_split(&mut self) -> (&mut T, LockToken<'_, L>) {
        (&mut self.inner, self.lock_token.token())
    }
}

impl<'a, L: Level, T: ?Sized + 'a> std::ops::Deref for MutexGuard<'a, L, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}
impl<'a, L: Level, T: ?Sized + 'a> std::ops::DerefMut for MutexGuard<'a, L, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.deref_mut()
    }
}

pub struct RwLock<L: Level, T> {
    inner: std::sync::RwLock<T>,
    _phantom: PhantomData<L>,
}

impl<L: Level, T: Default> Default for RwLock<L, T> {
    fn default() -> Self {
        Self {
            inner: Default::default(),
            _phantom: Default::default(),
        }
    }
}

/// A reader-writer lock
///
/// This type of lock allows a number of readers or at most one writer at any point in time.
/// The write portion of this lock typically allows modification of the underlying data (exclusive access)
/// and the read portion of this lock typically allows for read-only access (shared access).
///
/// The type parameter T represents the data that this lock protects. It is required that T satisfies
/// Send to be shared across threads and Sync to allow concurrent access through readers.
/// The RAII guards returned from the locking methods implement Deref (and DerefMut for the write methods)
/// to allow access to the contained of the lock.
impl<L: Level, T> RwLock<L, T> {
    /// Creates a new instance of an RwLock<T> which is unlocked.
    pub fn new(val: T) -> Self {
        Self {
            inner: std::sync::RwLock::new(val),
            _phantom: Default::default(),
        }
    }

    /// Consumes this RwLock, returning the underlying data.
    pub fn into_inner(self) -> T {
        self.inner.into_inner().unwrap()
    }

    /// Locks this RwLock with exclusive write access, blocking the current thread until it can be acquired.
    /// This function will not return while other writers or other readers currently have access to the lock.
    /// Returns an RAII guard which will drop the write access of this RwLock when dropped.
    pub fn write<'a, LP: Lower<L> + 'a>(
        &'a self,
        lock_token: LockToken<'a, LP>,
    ) -> std::sync::LockResult<RwLockWriteGuard<'a, L, T>> {
        Ok(RwLockWriteGuard {
            inner: self.inner.write().unwrap(),
            lock_token: LockToken::downgraded(lock_token),
        })
    }

    /// Locks this RwLock with shared read access, blocking the current thread until it can be acquired.
    ///
    /// The calling thread will be blocked until there are no more writers which hold the lock.
    /// There may be other readers currently inside the lock when this method returns.
    ///
    /// Note that attempts to recursively acquire a read lock on a RwLock when the current thread
    /// already holds one may result in a deadlock.
    ///
    /// Returns an RAII guard which will release this thread’s shared access once it is dropped.
    pub fn read<'a, LP: Lower<L> + 'a>(
        &'a self,
        lock_token: LockToken<'a, LP>,
    ) -> RwLockReadGuard<'a, L, T> {
        RwLockReadGuard {
            inner: self.inner.read().unwrap(),
            lock_token: LockToken::downgraded(lock_token),
        }
    }
}

/// RAII structure used to release the exclusive write access of a lock when dropped
pub struct RwLockWriteGuard<'a, L: Level, T> {
    inner: std::sync::RwLockWriteGuard<'a, T>,
    lock_token: LockToken<'a, L>,
}

impl<'a, L: Level, T> RwLockWriteGuard<'a, L, T> {
    /// Split the guard into two parts, the first a mutable reference to the held content
    /// the second a [`LockToken`] that can be used for further locking
    pub fn token_split(&mut self) -> (&mut T, LockToken<'_, L>) {
        (&mut self.inner, self.lock_token.token())
    }
}

impl<'a, L: Level, T> std::ops::Deref for RwLockWriteGuard<'a, L, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl<'a, L: Level, T> std::ops::DerefMut for RwLockWriteGuard<'a, L, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.deref_mut()
    }
}

/// RAII structure used to release the shared read access of a lock when dropped.
pub struct RwLockReadGuard<'a, L: Level, T> {
    inner: std::sync::RwLockReadGuard<'a, T>,
    lock_token: LockToken<'a, L>,
}

impl<'a, L: Level, T> RwLockReadGuard<'a, L, T> {
    /// Split the guard into two parts, the first a reference to the held content
    /// the second a [`LockToken`] that can be used for further locking
    pub fn token_split(&mut self) -> (&T, LockToken<'_, L>) {
        (&self.inner, self.lock_token.token())
    }
}

impl<'a, L: Level, T> std::ops::Deref for RwLockReadGuard<'a, L, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

/// An asynchronous `Mutex`-like type.
///
/// This type acts similarly to [`std::sync::Mutex`], with two major
/// differences: [`lock`] is an async method so does not block, and the lock
/// guard is designed to be held across `.await` points.
#[cfg(feature = "tokio")]
#[derive(Debug)]
pub struct AsyncMutex<L: Level, T> {
    inner: tokio::sync::Mutex<T>,
    _phantom: PhantomData<L>,
}

#[cfg(feature = "tokio")]
impl<L: Level, T> AsyncMutex<L, T> {
    /// Creates a new lock in an unlocked state ready for use.
    pub fn new(val: T) -> Self {
        Self {
            inner: tokio::sync::Mutex::new(val),
            _phantom: Default::default(),
        }
    }

    // Locks this mutex, causing the current task to yield until the lock has been acquired.
    // When the lock has been acquired, function returns a MutexGuard
    pub async fn lock<'a, LP: Lower<L> + 'a>(
        &'a self,
        lock_token: LockToken<'a, LP>,
    ) -> AsyncMutexGuard<'a, L, T> {
        AsyncMutexGuard {
            inner: self.inner.lock().await,
            lock_token: LockToken::downgraded(lock_token),
        }
    }

    /// Attempts to acquire the lock, and returns TryLockError if the lock is currently held somewhere else.
    ///
    /// The `lock_token` is held until the lock is released, to get a token for further locking
    /// call [`AsyncMutexGuard::token_split`]
    pub fn try_lock<'a, LP: Lower<L> + 'a>(
        &'a self,
        lock_token: LockToken<'a, LP>,
    ) -> Result<AsyncMutexGuard<'a, L, T>, tokio::sync::TryLockError> {
        self.inner.try_lock().map(|inner| AsyncMutexGuard {
            inner,
            lock_token: LockToken::downgraded(lock_token),
        })
    }

    /// Consumes this Mutex, returning the underlying data.
    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }
}

#[cfg(feature = "tokio")]
impl<L: Level, T: Default> Default for AsyncMutex<L, T> {
    fn default() -> Self {
        Self {
            inner: Default::default(),
            _phantom: Default::default(),
        }
    }
}

/// A handle to a held `Mutex`. The guard can be held across any `.await` point
/// as it is [`Send`].
///
/// As long as you have this guard, you have exclusive access to the underlying
/// `T`. The guard internally borrows the `Mutex`, so the mutex will not be
/// dropped while a guard exists.
///
/// The lock is automatically released whenever the guard is dropped, at which
/// point `lock` will succeed yet again.
#[cfg(feature = "tokio")]
pub struct AsyncMutexGuard<'a, L: Level, T: ?Sized + 'a> {
    inner: tokio::sync::MutexGuard<'a, T>,
    lock_token: LockToken<'a, L>,
}

#[cfg(feature = "tokio")]
impl<'a, L: Level, T: ?Sized + 'a> AsyncMutexGuard<'a, L, T> {
    /// Split the guard into two parts, the first a mutable reference to the held content
    /// the second a [`LockToken`] that can be used for further locking
    pub fn token_split(&mut self) -> (&mut T, LockToken<'_, L>) {
        (&mut self.inner, self.lock_token.token())
    }
}

#[cfg(feature = "tokio")]
impl<'a, L: Level, T: ?Sized + 'a> std::ops::Deref for AsyncMutexGuard<'a, L, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}
#[cfg(feature = "tokio")]
impl<'a, L: Level, T: ?Sized + 'a> std::ops::DerefMut for AsyncMutexGuard<'a, L, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.deref_mut()
    }
}

#[inline]
pub fn check_no_locks(_: LockToken<'_, L0>) {}
