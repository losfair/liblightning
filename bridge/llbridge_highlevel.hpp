#ifndef __LLBRIDGE_HIGHLEVEL_H_
#define __LLBRIDGE_HIGHLEVEL_H_

#include <cstdlib>
#include <cstdio>
#include <memory>
#include "llbridge.hpp"

namespace llbridge_highlevel {

using namespace liblightning;

class AsyncEntryCallback {
    public:
    AsyncEntryCallback() {}
    virtual ~AsyncEntryCallback() {}
    virtual void call(NotifyHandle *) {
        fprintf(stderr, "Calling an unoverrided async entry\n");
        abort();
    }
};

void ll_hl_async_enter(AsyncEntryCallback& cb) {
    ll_async_enter([](NotifyHandle *handle, const void *user_data) {
        AsyncEntryCallback *cb = (AsyncEntryCallback *) user_data;
        cb -> call(handle);
    }, (const void *) &cb);
}

class CoroutineEntryCallback {
    public:
    CoroutineEntryCallback() {}
    virtual ~CoroutineEntryCallback() {}
    virtual void call() {
        fprintf(stderr, "Calling an unoverrided coroutine entry\n");
        abort();
    }
};

void ll_hl_scheduler_start_coroutine(Scheduler *sch, CoroutineEntryCallback& entry) {
    ll_scheduler_start_coroutine(
        sch,
        [](const void *user_data) {
            CoroutineEntryCallback *entry = (CoroutineEntryCallback *) user_data;
            entry -> call();
        },
        (const void *) &entry
    );
}

}


#endif
