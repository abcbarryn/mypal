/* -*- Mode: C++; tab-width: 8; indent-tabs-mode: nil; c-basic-offset: 2 -*- */
/* vim: set ts=8 sts=2 et sw=2 tw=80: */
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef mozilla_mscom_COMPtrHolder_h
#define mozilla_mscom_COMPtrHolder_h

#include "mozilla/Assertions.h"
#include "mozilla/Attributes.h"
#include "mozilla/DebugOnly.h"
#include "mozilla/Move.h"
#include "mozilla/mscom/ProxyStream.h"
#include "mozilla/mscom/Ptr.h"
#if defined(MOZ_CONTENT_SANDBOX)
#include "mozilla/SandboxSettings.h"
#endif // defined(MOZ_CONTENT_SANDBOX)

namespace mozilla {
namespace mscom {

template<typename Interface, const IID& _IID>
class COMPtrHolder
{
public:
  typedef ProxyUniquePtr<Interface> COMPtrType;
  typedef COMPtrHolder<Interface, _IID> ThisType;

  COMPtrHolder() {}

  MOZ_IMPLICIT COMPtrHolder(decltype(nullptr))
  {
  }

  explicit COMPtrHolder(COMPtrType&& aPtr)
    : mPtr(Forward<COMPtrType>(aPtr))
  {
  }

  Interface* Get() const
  {
    return mPtr.get();
  }

  MOZ_MUST_USE Interface* Release()
  {
    return mPtr.release();
  }

  void Set(COMPtrType&& aPtr)
  {
    mPtr = Forward<COMPtrType>(aPtr);
  }

#if defined(MOZ_CONTENT_SANDBOX)
  // This method is const because we need to call it during IPC write, where
  // we are passed as a const argument. At higher sandboxing levels we need to
  // save this artifact from the serialization process for later deletion.
  void PreserveStream(RefPtr<IStream>&& aPtr) const
  {
    MOZ_ASSERT(!mMarshaledStream);
    mMarshaledStream = ToPreservedStreamPtr(Move(aPtr));
  }

  PreservedStreamPtr GetPreservedStream()
  {
    return Move(mMarshaledStream);
  }
#endif // defined(MOZ_CONTENT_SANDBOX)

  COMPtrHolder(const COMPtrHolder& aOther) = delete;

  COMPtrHolder(COMPtrHolder&& aOther)
    : mPtr(Move(aOther.mPtr))
  {
  }

  // COMPtrHolder is eventually added as a member of a struct that is declared
  // in IPDL. The generated C++ code for that IPDL struct includes copy
  // constructors and assignment operators that assume that all members are
  // copyable. I don't think that those copy constructors and operator= are
  // actually used by any generated code, but they are made available. Since no
  // move semantics are available, this terrible hack makes COMPtrHolder build
  // when used as a member of an IPDL struct.
  ThisType& operator=(const ThisType& aOther)
  {
    Set(Move(aOther.mPtr));
    return *this;
  }

  ThisType& operator=(ThisType&& aOther)
  {
    Set(Move(aOther.mPtr));
    return *this;
  }

  bool operator==(const ThisType& aOther) const
  {
    return mPtr == aOther.mPtr;
  }

  bool IsNull() const
  {
    return !mPtr;
  }

private:
  // This is mutable to facilitate the above operator= hack
  mutable COMPtrType mPtr;

#if defined(MOZ_CONTENT_SANDBOX)
  // This is mutable so that we may optionally store a reference to a marshaled
  // stream to be cleaned up later via PreserveStream().
  mutable PreservedStreamPtr mMarshaledStream;
#endif // defined(MOZ_CONTENT_SANDBOX)
};

} // namespace mscom
} // namespace mozilla

namespace IPC {

template<typename Interface, const IID& _IID>
struct ParamTraits<mozilla::mscom::COMPtrHolder<Interface, _IID>>
{
  typedef mozilla::mscom::COMPtrHolder<Interface, _IID> paramType;

  static void Write(Message* aMsg, const paramType& aParam)
  {
    mozilla::mscom::ProxyStream proxyStream(_IID, aParam.Get());
    int bufLen;
    const BYTE* buf = proxyStream.GetBuffer(bufLen);
    MOZ_ASSERT(buf || !bufLen);
    aMsg->WriteInt(bufLen);
    if (bufLen) {
      aMsg->WriteBytes(reinterpret_cast<const char*>(buf), bufLen);
    }

#if defined(MOZ_CONTENT_SANDBOX)
    if (XRE_IsParentProcess()) {
      static const bool sIsStreamPreservationNeeded =
        mozilla::GetEffectiveContentSandboxLevel() >= 3;
      if (sIsStreamPreservationNeeded) {
        /**
         * When we're sending a ProxyStream from parent to content and the
         * content sandboxing level is >= 3, content is unable to communicate
         * its releasing of its reference to the proxied object. We preserve the
         * marshaled proxy data here and later manually release it on content's
         * behalf.
         */
        RefPtr<IStream> stream(proxyStream.GetStream());
        aParam.PreserveStream(mozilla::Move(stream));
      }
    }
#endif // defined(MOZ_CONTENT_SANDBOX)
  }

  static bool Read(const Message* aMsg, PickleIterator* aIter, paramType* aResult)
  {
    int length;
    if (!aMsg->ReadLength(aIter, &length)) {
      return false;
    }

    mozilla::UniquePtr<BYTE[]> buf;
    if (length) {
      buf = mozilla::MakeUnique<BYTE[]>(length);
      if (!aMsg->ReadBytesInto(aIter, buf.get(), length)) {
        return false;
      }
    }

    mozilla::mscom::ProxyStream proxyStream(_IID, buf.get(), length);
    if (!proxyStream.IsValid()) {
      return false;
    }

    typename paramType::COMPtrType ptr;
    if (!proxyStream.GetInterface(mozilla::mscom::getter_AddRefs(ptr))) {
      return false;
    }

    aResult->Set(mozilla::Move(ptr));
    return true;
  }
};

} // namespace IPC

#endif // mozilla_mscom_COMPtrHolder_h
