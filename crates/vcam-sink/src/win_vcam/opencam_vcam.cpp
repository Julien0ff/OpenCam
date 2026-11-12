#include <windows.h>
#include <dshow.h>
#include <initguid.h>
#include <olectl.h>
#include <stdint.h>
#include <string.h>
#include <wchar.h>
#include <algorithm>

// {7A4B9C12-3E5F-4A8B-9C1D-2E3F4A5B6C7D}
DEFINE_GUID(CLSID_OpenCamVirtualCamera,
    0x7a4b9c12, 0x3e5f, 0x4a8b, 0x9c, 0x1d, 0x2e, 0x3f, 0x4a, 0x5b, 0x6c, 0x7d);

static const wchar_t* kFilterName = L"OpenCam Virtual Camera";
static const wchar_t* kSharedMemName = L"Local\\OpenCamVirtualCameraFrame";
static const uint32_t kSharedMagic = 0x4F43414D; // "OCAM"
static const uint32_t kSharedVersion = 1;
static const uint32_t kMaxFrameBytes = 3840 * 2160 * 2; // 4K YUY2 max

#pragma pack(push, 1)
struct OpenCamSharedHeader {
    uint32_t magic;
    uint32_t version;
    uint32_t width;
    uint32_t height;
    uint32_t fps;
    uint32_t active;
    uint64_t frame_seq;
};
#pragma pack(pop)

static LONG g_dllRefCount = 0;
static HMODULE g_hModule = nullptr;

static void FreeMediaTypeFields(AM_MEDIA_TYPE& mt) {
    if (mt.cbFormat != 0 && mt.pbFormat != nullptr) {
        CoTaskMemFree(mt.pbFormat);
        mt.pbFormat = nullptr;
        mt.cbFormat = 0;
    }
    if (mt.pUnk != nullptr) {
        mt.pUnk->Release();
        mt.pUnk = nullptr;
    }
}

static HRESULT CopyMediaTypeFields(AM_MEDIA_TYPE* dst, const AM_MEDIA_TYPE* src) {
    if (!dst || !src) return E_POINTER;
    *dst = *src;
    if (src->cbFormat != 0 && src->pbFormat != nullptr) {
        dst->pbFormat = (BYTE*)CoTaskMemAlloc(src->cbFormat);
        if (!dst->pbFormat) {
            dst->cbFormat = 0;
            return E_OUTOFMEMORY;
        }
        memcpy(dst->pbFormat, src->pbFormat, src->cbFormat);
    } else {
        dst->pbFormat = nullptr;
        dst->cbFormat = 0;
    }
    if (dst->pUnk != nullptr) {
        dst->pUnk->AddRef();
    }
    return S_OK;
}

static AM_MEDIA_TYPE* AllocCopyMediaType(const AM_MEDIA_TYPE* src) {
    AM_MEDIA_TYPE* pmt = (AM_MEDIA_TYPE*)CoTaskMemAlloc(sizeof(AM_MEDIA_TYPE));
    if (!pmt) return nullptr;
    if (FAILED(CopyMediaTypeFields(pmt, src))) {
        CoTaskMemFree(pmt);
        return nullptr;
    }
    return pmt;
}

static void DeleteAllocatedMediaType(AM_MEDIA_TYPE* pmt) {
    if (pmt) {
        FreeMediaTypeFields(*pmt);
        CoTaskMemFree(pmt);
    }
}

struct CapPreset {
    int width;
    int height;
    bool is_rgb24;
};

static const CapPreset kCapPresets[] = {
    { 1280, 720, false },
    { 1920, 1080, false },
    { 640, 480, false },
    { 1280, 720, true },
    { 1920, 1080, true },
};
static const int kNumCapPresets = sizeof(kCapPresets) / sizeof(kCapPresets[0]);

static HRESULT BuildVideoMediaType(int width, int height, bool is_rgb24, int fps, AM_MEDIA_TYPE* pmt) {
    if (!pmt || width <= 0 || height <= 0 || fps <= 0) return E_INVALIDARG;
    ZeroMemory(pmt, sizeof(AM_MEDIA_TYPE));

    VIDEOINFOHEADER* vih = (VIDEOINFOHEADER*)CoTaskMemAlloc(sizeof(VIDEOINFOHEADER));
    if (!vih) return E_OUTOFMEMORY;
    ZeroMemory(vih, sizeof(VIDEOINFOHEADER));

    DWORD bitCount = is_rgb24 ? 24 : 16;
    DWORD stride = is_rgb24 ? (((width * 3 + 3) / 4) * 4) : (DWORD)(width * 2);
    DWORD imageSize = stride * (DWORD)height;

    vih->AvgTimePerFrame = 10000000LL / fps;
    vih->dwBitRate = imageSize * 8 * (DWORD)fps;
    vih->bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    vih->bmiHeader.biWidth = width;
    vih->bmiHeader.biHeight = height;
    vih->bmiHeader.biPlanes = 1;
    vih->bmiHeader.biBitCount = (WORD)bitCount;
    vih->bmiHeader.biCompression = is_rgb24 ? BI_RGB : MAKEFOURCC('Y', 'U', 'Y', '2');
    vih->bmiHeader.biSizeImage = imageSize;

    pmt->majortype = MEDIATYPE_Video;
    pmt->subtype = is_rgb24 ? MEDIASUBTYPE_RGB24 : MEDIASUBTYPE_YUY2;
    pmt->bFixedSizeSamples = TRUE;
    pmt->bTemporalCompression = FALSE;
    pmt->lSampleSize = imageSize;
    pmt->formattype = FORMAT_VideoInfo;
    pmt->cbFormat = sizeof(VIDEOINFOHEADER);
    pmt->pbFormat = (BYTE*)vih;
    return S_OK;
}

static bool IsSupportedVideoMediaType(const AM_MEDIA_TYPE* pmt) {
    if (!pmt) return false;
    if (pmt->majortype != MEDIATYPE_Video) return false;
    if (pmt->subtype != MEDIASUBTYPE_YUY2 && pmt->subtype != MEDIASUBTYPE_RGB24) return false;
    if (pmt->formattype != FORMAT_VideoInfo || pmt->cbFormat < sizeof(VIDEOINFOHEADER) || !pmt->pbFormat) {
        return false;
    }
    const VIDEOINFOHEADER* vih = (const VIDEOINFOHEADER*)pmt->pbFormat;
    if (vih->bmiHeader.biWidth <= 0 || vih->bmiHeader.biHeight == 0) return false;
    return true;
}

static inline uint8_t ClampU8(int v) {
    return (uint8_t)(v < 0 ? 0 : (v > 255 ? 255 : v));
}

class COpenCamFilter;

class COpenCamEnumMediaTypes : public IEnumMediaTypes {
public:
    COpenCamEnumMediaTypes(int index = 0) : m_ref(1), m_index(index) {
        InterlockedIncrement(&g_dllRefCount);
    }
    ~COpenCamEnumMediaTypes() {
        InterlockedDecrement(&g_dllRefCount);
    }

    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID riid, void** ppv) override {
        if (!ppv) return E_POINTER;
        if (riid == IID_IUnknown || riid == IID_IEnumMediaTypes) {
            *ppv = static_cast<IEnumMediaTypes*>(this);
            AddRef();
            return S_OK;
        }
        *ppv = nullptr;
        return E_NOINTERFACE;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return (ULONG)InterlockedIncrement(&m_ref); }
    ULONG STDMETHODCALLTYPE Release() override {
        ULONG r = (ULONG)InterlockedDecrement(&m_ref);
        if (r == 0) delete this;
        return r;
    }

    HRESULT STDMETHODCALLTYPE Next(ULONG cMediaTypes, AM_MEDIA_TYPE** ppMediaTypes, ULONG* pcFetched) override {
        if (!ppMediaTypes) return E_POINTER;
        ULONG fetched = 0;
        while (fetched < cMediaTypes && m_index < kNumCapPresets) {
            AM_MEDIA_TYPE mt = {};
            const CapPreset& p = kCapPresets[m_index];
            if (SUCCEEDED(BuildVideoMediaType(p.width, p.height, p.is_rgb24, 30, &mt))) {
                ppMediaTypes[fetched] = AllocCopyMediaType(&mt);
                FreeMediaTypeFields(mt);
                if (!ppMediaTypes[fetched]) return E_OUTOFMEMORY;
                fetched++;
                m_index++;
            } else {
                break;
            }
        }
        if (pcFetched) *pcFetched = fetched;
        return (fetched == cMediaTypes) ? S_OK : S_FALSE;
    }
    HRESULT STDMETHODCALLTYPE Skip(ULONG cMediaTypes) override {
        m_index += (int)cMediaTypes;
        if (m_index > kNumCapPresets) {
            m_index = kNumCapPresets;
            return S_FALSE;
        }
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE Reset() override {
        m_index = 0;
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE Clone(IEnumMediaTypes** ppEnum) override {
        if (!ppEnum) return E_POINTER;
        *ppEnum = new COpenCamEnumMediaTypes(m_index);
        return (*ppEnum) ? S_OK : E_OUTOFMEMORY;
    }

private:
    LONG m_ref;
    int m_index;
};

class COpenCamPin : public IPin, public IAMStreamConfig, public IKsPropertySet {
public:
    COpenCamPin(COpenCamFilter* pFilter);
    ~COpenCamPin();

    // IUnknown
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID riid, void** ppv) override;
    ULONG STDMETHODCALLTYPE AddRef() override;
    ULONG STDMETHODCALLTYPE Release() override;

    // IPin
    HRESULT STDMETHODCALLTYPE Connect(IPin* pReceivePin, const AM_MEDIA_TYPE* pmt) override;
    HRESULT STDMETHODCALLTYPE ReceiveConnection(IPin* pConnector, const AM_MEDIA_TYPE* pmt) override;
    HRESULT STDMETHODCALLTYPE Disconnect() override;
    HRESULT STDMETHODCALLTYPE ConnectedTo(IPin** pPin) override;
    HRESULT STDMETHODCALLTYPE ConnectionMediaType(AM_MEDIA_TYPE* pmt) override;
    HRESULT STDMETHODCALLTYPE QueryPinInfo(PIN_INFO* pInfo) override;
    HRESULT STDMETHODCALLTYPE QueryDirection(PIN_DIRECTION* pPinDir) override;
    HRESULT STDMETHODCALLTYPE QueryId(LPWSTR* Id) override;
    HRESULT STDMETHODCALLTYPE QueryAccept(const AM_MEDIA_TYPE* pmt) override;
    HRESULT STDMETHODCALLTYPE EnumMediaTypes(IEnumMediaTypes** ppEnum) override;
    HRESULT STDMETHODCALLTYPE QueryInternalConnections(IPin** apPin, ULONG* nPin) override;
    HRESULT STDMETHODCALLTYPE EndOfStream() override;
    HRESULT STDMETHODCALLTYPE BeginFlush() override;
    HRESULT STDMETHODCALLTYPE EndFlush() override;
    HRESULT STDMETHODCALLTYPE NewSegment(REFERENCE_TIME tStart, REFERENCE_TIME tStop, double dRate) override;

    // IAMStreamConfig
    HRESULT STDMETHODCALLTYPE SetFormat(AM_MEDIA_TYPE* pmt) override;
    HRESULT STDMETHODCALLTYPE GetFormat(AM_MEDIA_TYPE** ppmt) override;
    HRESULT STDMETHODCALLTYPE GetNumberOfCapabilities(int* piCount, int* piSize) override;
    HRESULT STDMETHODCALLTYPE GetStreamCaps(int iIndex, AM_MEDIA_TYPE** ppmt, BYTE* pSCC) override;

    // IKsPropertySet
    HRESULT STDMETHODCALLTYPE Set(REFGUID guidPropSet, DWORD dwPropID, LPVOID pInstanceData, DWORD cbInstanceData, LPVOID pPropData, DWORD cbPropData) override;
    HRESULT STDMETHODCALLTYPE Get(REFGUID guidPropSet, DWORD dwPropID, LPVOID pInstanceData, DWORD cbInstanceData, LPVOID pPropData, DWORD cbPropData, DWORD* pcbReturned) override;
    HRESULT STDMETHODCALLTYPE QuerySupported(REFGUID guidPropSet, DWORD dwPropID, DWORD* pTypeSupport) override;

    void StartStreaming();
    void StopStreaming();

private:
    static DWORD WINAPI StreamThreadProc(LPVOID param);
    void RunStreamLoop();
    void FillFrame(BYTE* dst, long dstMaxLen, long* outLen);

    COpenCamFilter* m_pFilter;
    CRITICAL_SECTION m_cs;
    AM_MEDIA_TYPE m_mt;
    IPin* m_pConnectedPin;
    IMemInputPin* m_pInputPin;
    IMemAllocator* m_pAllocator;
    HANDLE m_hStopEvent;
    HANDLE m_hThread;
};

class COpenCamEnumPins : public IEnumPins {
public:
    COpenCamEnumPins(COpenCamPin* pPin, int index = 0);
    ~COpenCamEnumPins();

    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID riid, void** ppv) override {
        if (!ppv) return E_POINTER;
        if (riid == IID_IUnknown || riid == IID_IEnumPins) {
            *ppv = static_cast<IEnumPins*>(this);
            AddRef();
            return S_OK;
        }
        *ppv = nullptr;
        return E_NOINTERFACE;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return (ULONG)InterlockedIncrement(&m_ref); }
    ULONG STDMETHODCALLTYPE Release() override {
        ULONG r = (ULONG)InterlockedDecrement(&m_ref);
        if (r == 0) delete this;
        return r;
    }

    HRESULT STDMETHODCALLTYPE Next(ULONG cPins, IPin** ppPins, ULONG* pcFetched) override {
        if (!ppPins) return E_POINTER;
        ULONG fetched = 0;
        if (cPins > 0 && m_index == 0) {
            ppPins[0] = static_cast<IPin*>(m_pPin);
            ppPins[0]->AddRef();
            fetched = 1;
            m_index = 1;
        }
        if (pcFetched) *pcFetched = fetched;
        return (fetched == cPins) ? S_OK : S_FALSE;
    }
    HRESULT STDMETHODCALLTYPE Skip(ULONG cPins) override {
        m_index += (int)cPins;
        if (m_index > 1) {
            m_index = 1;
            return S_FALSE;
        }
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE Reset() override {
        m_index = 0;
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE Clone(IEnumPins** ppEnum) override {
        if (!ppEnum) return E_POINTER;
        *ppEnum = new COpenCamEnumPins(m_pPin, m_index);
        return (*ppEnum) ? S_OK : E_OUTOFMEMORY;
    }

private:
    LONG m_ref;
    COpenCamPin* m_pPin;
    int m_index;
};

class COpenCamFilter : public IBaseFilter, public IAMFilterMiscFlags {
public:
    COpenCamFilter()
        : m_ref(1), m_state(State_Stopped), m_pGraph(nullptr), m_pClock(nullptr) {
        InterlockedIncrement(&g_dllRefCount);
        InitializeCriticalSection(&m_cs);
        m_name[0] = 0;
        m_pPin = new COpenCamPin(this);
    }

    ~COpenCamFilter() {
        if (m_pPin) {
            m_pPin->StopStreaming();
            delete m_pPin;
            m_pPin = nullptr;
        }
        if (m_pClock) {
            m_pClock->Release();
            m_pClock = nullptr;
        }
        DeleteCriticalSection(&m_cs);
        InterlockedDecrement(&g_dllRefCount);
    }

    // IUnknown
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID riid, void** ppv) override {
        if (!ppv) return E_POINTER;
        if (riid == IID_IUnknown || riid == IID_IPersist || riid == IID_IMediaFilter || riid == IID_IBaseFilter) {
            *ppv = static_cast<IBaseFilter*>(this);
            AddRef();
            return S_OK;
        }
        if (riid == IID_IAMFilterMiscFlags) {
            *ppv = static_cast<IAMFilterMiscFlags*>(this);
            AddRef();
            return S_OK;
        }
        *ppv = nullptr;
        return E_NOINTERFACE;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return (ULONG)InterlockedIncrement(&m_ref); }
    ULONG STDMETHODCALLTYPE Release() override {
        ULONG r = (ULONG)InterlockedDecrement(&m_ref);
        if (r == 0) delete this;
        return r;
    }

    // IPersist
    HRESULT STDMETHODCALLTYPE GetClassID(CLSID* pClassID) override {
        if (!pClassID) return E_POINTER;
        *pClassID = CLSID_OpenCamVirtualCamera;
        return S_OK;
    }

    // IMediaFilter
    HRESULT STDMETHODCALLTYPE Stop() override {
        EnterCriticalSection(&m_cs);
        m_state = State_Stopped;
        LeaveCriticalSection(&m_cs);
        m_pPin->StopStreaming();
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE Pause() override {
        EnterCriticalSection(&m_cs);
        FILTER_STATE prev = m_state;
        m_state = State_Paused;
        LeaveCriticalSection(&m_cs);
        if (prev == State_Stopped) {
            m_pPin->StartStreaming();
        }
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE Run(REFERENCE_TIME) override {
        EnterCriticalSection(&m_cs);
        FILTER_STATE prev = m_state;
        m_state = State_Running;
        LeaveCriticalSection(&m_cs);
        if (prev == State_Stopped) {
            m_pPin->StartStreaming();
        }
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE GetState(DWORD, FILTER_STATE* State) override {
        if (!State) return E_POINTER;
        EnterCriticalSection(&m_cs);
        *State = m_state;
        LeaveCriticalSection(&m_cs);
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE SetSyncSource(IReferenceClock* pClock) override {
        EnterCriticalSection(&m_cs);
        if (m_pClock) m_pClock->Release();
        m_pClock = pClock;
        if (m_pClock) m_pClock->AddRef();
        LeaveCriticalSection(&m_cs);
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE GetSyncSource(IReferenceClock** pClock) override {
        if (!pClock) return E_POINTER;
        EnterCriticalSection(&m_cs);
        *pClock = m_pClock;
        if (m_pClock) m_pClock->AddRef();
        LeaveCriticalSection(&m_cs);
        return S_OK;
    }

    // IBaseFilter
    HRESULT STDMETHODCALLTYPE EnumPins(IEnumPins** ppEnum) override {
        if (!ppEnum) return E_POINTER;
        *ppEnum = new COpenCamEnumPins(m_pPin, 0);
        return (*ppEnum) ? S_OK : E_OUTOFMEMORY;
    }
    HRESULT STDMETHODCALLTYPE FindPin(LPCWSTR Id, IPin** ppPin) override {
        if (!ppPin) return E_POINTER;
        if (Id && wcscmp(Id, L"Capture") == 0) {
            *ppPin = static_cast<IPin*>(m_pPin);
            (*ppPin)->AddRef();
            return S_OK;
        }
        *ppPin = nullptr;
        return VFW_E_NOT_FOUND;
    }
    HRESULT STDMETHODCALLTYPE QueryFilterInfo(FILTER_INFO* pInfo) override {
        if (!pInfo) return E_POINTER;
        EnterCriticalSection(&m_cs);
        if (m_name[0] != 0) {
            wcsncpy_s(pInfo->achName, m_name, _TRUNCATE);
        } else {
            wcsncpy_s(pInfo->achName, kFilterName, _TRUNCATE);
        }
        pInfo->pGraph = m_pGraph;
        if (m_pGraph) m_pGraph->AddRef();
        LeaveCriticalSection(&m_cs);
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE JoinFilterGraph(IFilterGraph* pGraph, LPCWSTR pName) override {
        EnterCriticalSection(&m_cs);
        m_pGraph = pGraph;
        if (pName) {
            wcsncpy_s(m_name, pName, _TRUNCATE);
        } else {
            m_name[0] = 0;
        }
        LeaveCriticalSection(&m_cs);
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE QueryVendorInfo(LPWSTR* pVendorInfo) override {
        if (pVendorInfo) *pVendorInfo = nullptr;
        return E_NOTIMPL;
    }

    // IAMFilterMiscFlags
    ULONG STDMETHODCALLTYPE GetMiscFlags() override {
        return AM_FILTER_MISC_FLAGS_IS_SOURCE;
    }

    FILTER_STATE CurrentState() {
        EnterCriticalSection(&m_cs);
        FILTER_STATE s = m_state;
        LeaveCriticalSection(&m_cs);
        return s;
    }

private:
    LONG m_ref;
    CRITICAL_SECTION m_cs;
    FILTER_STATE m_state;
    IFilterGraph* m_pGraph;
    IReferenceClock* m_pClock;
    WCHAR m_name[MAX_FILTER_NAME];
    COpenCamPin* m_pPin;
};

COpenCamEnumPins::COpenCamEnumPins(COpenCamPin* pPin, int index)
    : m_ref(1), m_pPin(pPin), m_index(index) {
    InterlockedIncrement(&g_dllRefCount);
    if (m_pPin) m_pPin->AddRef();
}

COpenCamEnumPins::~COpenCamEnumPins() {
    if (m_pPin) m_pPin->Release();
    InterlockedDecrement(&g_dllRefCount);
}

COpenCamPin::COpenCamPin(COpenCamFilter* pFilter)
    : m_pFilter(pFilter),
      m_pConnectedPin(nullptr),
      m_pInputPin(nullptr),
      m_pAllocator(nullptr),
      m_hStopEvent(nullptr),
      m_hThread(nullptr) {
    InitializeCriticalSection(&m_cs);
    ZeroMemory(&m_mt, sizeof(m_mt));
    BuildVideoMediaType(1280, 720, false, 30, &m_mt);
}

COpenCamPin::~COpenCamPin() {
    StopStreaming();
    Disconnect();
    FreeMediaTypeFields(m_mt);
    DeleteCriticalSection(&m_cs);
}

HRESULT STDMETHODCALLTYPE COpenCamPin::QueryInterface(REFIID riid, void** ppv) {
    if (!ppv) return E_POINTER;
    if (riid == IID_IUnknown || riid == IID_IPin) {
        *ppv = static_cast<IPin*>(this);
        AddRef();
        return S_OK;
    }
    if (riid == IID_IAMStreamConfig) {
        *ppv = static_cast<IAMStreamConfig*>(this);
        AddRef();
        return S_OK;
    }
    if (riid == IID_IKsPropertySet) {
        *ppv = static_cast<IKsPropertySet*>(this);
        AddRef();
        return S_OK;
    }
    *ppv = nullptr;
    return E_NOINTERFACE;
}

ULONG STDMETHODCALLTYPE COpenCamPin::AddRef() { return m_pFilter->AddRef(); }
ULONG STDMETHODCALLTYPE COpenCamPin::Release() { return m_pFilter->Release(); }

HRESULT STDMETHODCALLTYPE COpenCamPin::Connect(IPin* pReceivePin, const AM_MEDIA_TYPE* pmt) {
    if (!pReceivePin) return E_POINTER;

    EnterCriticalSection(&m_cs);
    if (m_pConnectedPin) {
        LeaveCriticalSection(&m_cs);
        return VFW_E_ALREADY_CONNECTED;
    }

    AM_MEDIA_TYPE chosen = {};
    if (pmt) {
        if (!IsSupportedVideoMediaType(pmt)) {
            LeaveCriticalSection(&m_cs);
            return VFW_E_TYPE_NOT_ACCEPTED;
        }
        CopyMediaTypeFields(&chosen, pmt);
    } else {
        CopyMediaTypeFields(&chosen, &m_mt);
    }

    IMemInputPin* pInputPin = nullptr;
    HRESULT hr = pReceivePin->QueryInterface(IID_IMemInputPin, (void**)&pInputPin);
    if (FAILED(hr)) {
        FreeMediaTypeFields(chosen);
        LeaveCriticalSection(&m_cs);
        return hr;
    }

    hr = pReceivePin->ReceiveConnection(static_cast<IPin*>(this), &chosen);
    if (FAILED(hr)) {
        pInputPin->Release();
        FreeMediaTypeFields(chosen);
        LeaveCriticalSection(&m_cs);
        return hr;
    }

    IMemAllocator* pAlloc = nullptr;
    hr = pInputPin->GetAllocator(&pAlloc);
    if (FAILED(hr) || !pAlloc) {
        hr = CoCreateInstance(CLSID_MemoryAllocator, nullptr, CLSCTX_INPROC_SERVER, IID_IMemAllocator, (void**)&pAlloc);
        if (FAILED(hr) || !pAlloc) {
            pInputPin->Release();
            FreeMediaTypeFields(chosen);
            LeaveCriticalSection(&m_cs);
            return E_FAIL;
        }
    }

    ALLOCATOR_PROPERTIES req = {};
    req.cBuffers = 2;
    req.cbBuffer = (chosen.lSampleSize > 0) ? (long)chosen.lSampleSize : (1920 * 1080 * 3);
    req.cbAlign = 1;
    req.cbPrefix = 0;
    ALLOCATOR_PROPERTIES actual = {};
    hr = pAlloc->SetProperties(&req, &actual);
    if (FAILED(hr)) {
        pAlloc->Release();
        pInputPin->Release();
        FreeMediaTypeFields(chosen);
        LeaveCriticalSection(&m_cs);
        return hr;
    }

    hr = pInputPin->NotifyAllocator(pAlloc, FALSE);
    if (FAILED(hr)) {
        pAlloc->Release();
        pInputPin->Release();
        FreeMediaTypeFields(chosen);
        LeaveCriticalSection(&m_cs);
        return hr;
    }

    FreeMediaTypeFields(m_mt);
    m_mt = chosen;
    m_pConnectedPin = pReceivePin;
    m_pConnectedPin->AddRef();
    m_pInputPin = pInputPin;
    m_pAllocator = pAlloc;

    LeaveCriticalSection(&m_cs);
    return S_OK;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::ReceiveConnection(IPin*, const AM_MEDIA_TYPE*) {
    return E_UNEXPECTED;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::Disconnect() {
    StopStreaming();
    EnterCriticalSection(&m_cs);
    if (m_pAllocator) {
        m_pAllocator->Decommit();
        m_pAllocator->Release();
        m_pAllocator = nullptr;
    }
    if (m_pInputPin) {
        m_pInputPin->Release();
        m_pInputPin = nullptr;
    }
    if (m_pConnectedPin) {
        m_pConnectedPin->Release();
        m_pConnectedPin = nullptr;
    }
    LeaveCriticalSection(&m_cs);
    return S_OK;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::ConnectedTo(IPin** pPin) {
    if (!pPin) return E_POINTER;
    EnterCriticalSection(&m_cs);
    *pPin = m_pConnectedPin;
    if (m_pConnectedPin) {
        m_pConnectedPin->AddRef();
        LeaveCriticalSection(&m_cs);
        return S_OK;
    }
    LeaveCriticalSection(&m_cs);
    return VFW_E_NOT_CONNECTED;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::ConnectionMediaType(AM_MEDIA_TYPE* pmt) {
    if (!pmt) return E_POINTER;
    EnterCriticalSection(&m_cs);
    if (!m_pConnectedPin) {
        LeaveCriticalSection(&m_cs);
        return VFW_E_NOT_CONNECTED;
    }
    HRESULT hr = CopyMediaTypeFields(pmt, &m_mt);
    LeaveCriticalSection(&m_cs);
    return hr;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::QueryPinInfo(PIN_INFO* pInfo) {
    if (!pInfo) return E_POINTER;
    pInfo->pFilter = static_cast<IBaseFilter*>(m_pFilter);
    if (pInfo->pFilter) pInfo->pFilter->AddRef();
    pInfo->dir = PINDIR_OUTPUT;
    wcscpy_s(pInfo->achName, L"Capture");
    return S_OK;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::QueryDirection(PIN_DIRECTION* pPinDir) {
    if (!pPinDir) return E_POINTER;
    *pPinDir = PINDIR_OUTPUT;
    return S_OK;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::QueryId(LPWSTR* Id) {
    if (!Id) return E_POINTER;
    LPWSTR str = (LPWSTR)CoTaskMemAlloc(sizeof(L"Capture"));
    if (!str) return E_OUTOFMEMORY;
    wcscpy_s(str, 8, L"Capture");
    *Id = str;
    return S_OK;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::QueryAccept(const AM_MEDIA_TYPE* pmt) {
    return IsSupportedVideoMediaType(pmt) ? S_OK : S_FALSE;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::EnumMediaTypes(IEnumMediaTypes** ppEnum) {
    if (!ppEnum) return E_POINTER;
    *ppEnum = new COpenCamEnumMediaTypes(0);
    return (*ppEnum) ? S_OK : E_OUTOFMEMORY;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::QueryInternalConnections(IPin**, ULONG*) { return E_NOTIMPL; }
HRESULT STDMETHODCALLTYPE COpenCamPin::EndOfStream() { return E_UNEXPECTED; }
HRESULT STDMETHODCALLTYPE COpenCamPin::BeginFlush() { return E_UNEXPECTED; }
HRESULT STDMETHODCALLTYPE COpenCamPin::EndFlush() { return E_UNEXPECTED; }
HRESULT STDMETHODCALLTYPE COpenCamPin::NewSegment(REFERENCE_TIME, REFERENCE_TIME, double) { return S_OK; }

HRESULT STDMETHODCALLTYPE COpenCamPin::SetFormat(AM_MEDIA_TYPE* pmt) {
    if (!pmt) return E_POINTER;
    if (!IsSupportedVideoMediaType(pmt)) return VFW_E_INVALIDMEDIATYPE;
    EnterCriticalSection(&m_cs);
    if (m_pConnectedPin) {
        LeaveCriticalSection(&m_cs);
        return VFW_E_WRONG_STATE;
    }
    FreeMediaTypeFields(m_mt);
    HRESULT hr = CopyMediaTypeFields(&m_mt, pmt);
    LeaveCriticalSection(&m_cs);
    return hr;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::GetFormat(AM_MEDIA_TYPE** ppmt) {
    if (!ppmt) return E_POINTER;
    EnterCriticalSection(&m_cs);
    *ppmt = AllocCopyMediaType(&m_mt);
    LeaveCriticalSection(&m_cs);
    return (*ppmt) ? S_OK : E_OUTOFMEMORY;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::GetNumberOfCapabilities(int* piCount, int* piSize) {
    if (!piCount || !piSize) return E_POINTER;
    *piCount = kNumCapPresets;
    *piSize = sizeof(VIDEO_STREAM_CONFIG_CAPS);
    return S_OK;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::GetStreamCaps(int iIndex, AM_MEDIA_TYPE** ppmt, BYTE* pSCC) {
    if (!ppmt || !pSCC) return E_POINTER;
    if (iIndex < 0 || iIndex >= kNumCapPresets) return S_FALSE;

    const CapPreset& p = kCapPresets[iIndex];
    AM_MEDIA_TYPE mt = {};
    HRESULT hr = BuildVideoMediaType(p.width, p.height, p.is_rgb24, 30, &mt);
    if (FAILED(hr)) return hr;

    *ppmt = AllocCopyMediaType(&mt);
    FreeMediaTypeFields(mt);
    if (!*ppmt) return E_OUTOFMEMORY;

    VIDEO_STREAM_CONFIG_CAPS* scc = (VIDEO_STREAM_CONFIG_CAPS*)pSCC;
    ZeroMemory(scc, sizeof(VIDEO_STREAM_CONFIG_CAPS));
    scc->guid = FORMAT_VideoInfo;
    scc->VideoStandard = AnalogVideo_None;
    scc->InputSize.cx = p.width;
    scc->InputSize.cy = p.height;
    scc->MinCroppingSize.cx = p.width;
    scc->MinCroppingSize.cy = p.height;
    scc->MaxCroppingSize.cx = p.width;
    scc->MaxCroppingSize.cy = p.height;
    scc->CropGranularityX = 1;
    scc->CropGranularityY = 1;
    scc->CropAlignX = 1;
    scc->CropAlignY = 1;
    scc->MinOutputSize.cx = p.width;
    scc->MinOutputSize.cy = p.height;
    scc->MaxOutputSize.cx = p.width;
    scc->MaxOutputSize.cy = p.height;
    scc->OutputGranularityX = 1;
    scc->OutputGranularityY = 1;
    scc->MinFrameInterval = 10000000LL / 60;
    scc->MaxFrameInterval = 10000000LL / 15;
    scc->MinBitsPerSecond = p.width * p.height * 16 * 15;
    scc->MaxBitsPerSecond = p.width * p.height * 24 * 60;
    return S_OK;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::Set(REFGUID, DWORD, LPVOID, DWORD, LPVOID, DWORD) {
    return E_NOTIMPL;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::Get(
    REFGUID guidPropSet,
    DWORD dwPropID,
    LPVOID,
    DWORD,
    LPVOID pPropData,
    DWORD cbPropData,
    DWORD* pcbReturned) {
    if (guidPropSet != AMPROPSETID_Pin) return E_PROP_SET_UNSUPPORTED;
    if (dwPropID != AMPROPERTY_PIN_CATEGORY) return E_PROP_ID_UNSUPPORTED;
    if (pcbReturned) *pcbReturned = sizeof(GUID);
    if (!pPropData) return S_OK;
    if (cbPropData < sizeof(GUID)) return E_UNEXPECTED;
    *(GUID*)pPropData = PIN_CATEGORY_CAPTURE;
    return S_OK;
}

HRESULT STDMETHODCALLTYPE COpenCamPin::QuerySupported(REFGUID guidPropSet, DWORD dwPropID, DWORD* pTypeSupport) {
    if (guidPropSet != AMPROPSETID_Pin) return E_PROP_SET_UNSUPPORTED;
    if (dwPropID != AMPROPERTY_PIN_CATEGORY) return E_PROP_ID_UNSUPPORTED;
    if (pTypeSupport) *pTypeSupport = KSPROPERTY_SUPPORT_GET;
    return S_OK;
}

void COpenCamPin::StartStreaming() {
    EnterCriticalSection(&m_cs);
    if (m_hThread || !m_pConnectedPin || !m_pAllocator || !m_pInputPin) {
        LeaveCriticalSection(&m_cs);
        return;
    }
    m_pAllocator->Commit();
    m_hStopEvent = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    m_hThread = CreateThread(nullptr, 0, StreamThreadProc, this, 0, nullptr);
    LeaveCriticalSection(&m_cs);
}

void COpenCamPin::StopStreaming() {
    HANDLE hThread = nullptr;
    HANDLE hStop = nullptr;
    EnterCriticalSection(&m_cs);
    hThread = m_hThread;
    hStop = m_hStopEvent;
    m_hThread = nullptr;
    m_hStopEvent = nullptr;
    LeaveCriticalSection(&m_cs);

    if (hStop) SetEvent(hStop);
    if (hThread) {
        WaitForSingleObject(hThread, 3000);
        CloseHandle(hThread);
    }
    if (hStop) CloseHandle(hStop);

    EnterCriticalSection(&m_cs);
    if (m_pAllocator) m_pAllocator->Decommit();
    LeaveCriticalSection(&m_cs);
}

DWORD WINAPI COpenCamPin::StreamThreadProc(LPVOID param) {
    ((COpenCamPin*)param)->RunStreamLoop();
    return 0;
}

void COpenCamPin::FillFrame(BYTE* dst, long dstMaxLen, long* outLen) {
    EnterCriticalSection(&m_cs);
    const VIDEOINFOHEADER* vih = (const VIDEOINFOHEADER*)m_mt.pbFormat;
    int dstW = vih ? vih->bmiHeader.biWidth : 1280;
    int dstH = vih ? abs(vih->bmiHeader.biHeight) : 720;
    bool topDown = vih ? (vih->bmiHeader.biHeight < 0) : false;
    bool isRgb24 = (m_mt.subtype == MEDIASUBTYPE_RGB24);
    LeaveCriticalSection(&m_cs);

    long dstStride = isRgb24 ? (((dstW * 3 + 3) / 4) * 4) : (dstW * 2);
    long needed = dstStride * dstH;
    if (needed > dstMaxLen) {
        *outLen = 0;
        return;
    }
    *outLen = needed;

    bool copiedFromShared = false;
    HANDLE hMap = OpenFileMappingW(FILE_MAP_READ, FALSE, kSharedMemName);
    if (hMap) {
        const uint8_t* base = (const uint8_t*)MapViewOfFile(hMap, FILE_MAP_READ, 0, 0, 0);
        if (base) {
            const OpenCamSharedHeader* hdr = (const OpenCamSharedHeader*)base;
            if (hdr->magic == kSharedMagic && hdr->version == kSharedVersion && hdr->active == 1 &&
                hdr->width > 0 && hdr->height > 0 &&
                (uint64_t)hdr->width * (uint64_t)hdr->height * 2ULL <= kMaxFrameBytes) {
                const uint8_t* src = base + sizeof(OpenCamSharedHeader);
                int srcW = (int)hdr->width;
                int srcH = (int)hdr->height;
                int srcStride = srcW * 2;

                if (!isRgb24 && srcW == dstW && srcH == dstH) {
                    memcpy(dst, src, (size_t)needed);
                } else if (!isRgb24) {
                    // Nearest-neighbor YUY2 scaling (macro-pixel aligned)
                    for (int y = 0; y < dstH; y++) {
                        int sy = (y * srcH) / dstH;
                        const uint8_t* sRow = src + sy * srcStride;
                        uint8_t* dRow = dst + y * dstStride;
                        for (int x = 0; x + 1 < dstW; x += 2) {
                            int sx = ((x * srcW) / dstW) & ~1;
                            if (sx + 3 >= srcStride) sx = (srcW >= 2) ? (srcW - 2) * 2 : 0;
                            else sx *= 2;
                            dRow[x * 2 + 0] = sRow[sx + 0];
                            dRow[x * 2 + 1] = sRow[sx + 1];
                            dRow[x * 2 + 2] = sRow[sx + 2];
                            dRow[x * 2 + 3] = sRow[sx + 3];
                        }
                    }
                } else {
                    // YUY2 -> RGB24 (with nearest-neighbor scaling and optional vertical flip)
                    for (int y = 0; y < dstH; y++) {
                        int sy = (y * srcH) / dstH;
                        int dy = topDown ? y : (dstH - 1 - y);
                        const uint8_t* sRow = src + sy * srcStride;
                        uint8_t* dRow = dst + dy * dstStride;
                        for (int x = 0; x < dstW; x++) {
                            int sx = (x * srcW) / dstW;
                            int pair = (sx & ~1) * 2;
                            int yVal = (sx & 1) ? sRow[pair + 2] : sRow[pair + 0];
                            int uVal = sRow[pair + 1] - 128;
                            int vVal = sRow[pair + 3] - 128;
                            int c = yVal - 16;
                            int r = (298 * c + 409 * vVal + 128) >> 8;
                            int g = (298 * c - 100 * uVal - 208 * vVal + 128) >> 8;
                            int b = (298 * c + 516 * uVal + 128) >> 8;
                            dRow[x * 3 + 0] = ClampU8(b);
                            dRow[x * 3 + 1] = ClampU8(g);
                            dRow[x * 3 + 2] = ClampU8(r);
                        }
                    }
                }
                copiedFromShared = true;
            }
            UnmapViewOfFile(base);
        }
        CloseHandle(hMap);
    }

    if (!copiedFromShared) {
        if (isRgb24) {
            memset(dst, 20, (size_t)needed);
        } else {
            // Neutral dark YUY2 frame (Y=20, U=128, V=128)
            for (long i = 0; i + 3 < needed; i += 4) {
                dst[i + 0] = 20;
                dst[i + 1] = 128;
                dst[i + 2] = 20;
                dst[i + 3] = 128;
            }
        }
    }
}

void COpenCamPin::RunStreamLoop() {
    const REFERENCE_TIME frameDuration = 10000000LL / 30; // 30 fps
    LONGLONG frameIdx = 0;

    while (WaitForSingleObject(m_hStopEvent, 33) == WAIT_TIMEOUT) {
        if (m_pFilter->CurrentState() == State_Paused) {
            continue;
        }

        IMediaSample* pSample = nullptr;
        HRESULT hr = m_pAllocator->GetBuffer(&pSample, nullptr, nullptr, 0);
        if (FAILED(hr) || !pSample) {
            continue;
        }

        BYTE* pBuf = nullptr;
        if (SUCCEEDED(pSample->GetPointer(&pBuf)) && pBuf) {
            long len = 0;
            FillFrame(pBuf, pSample->GetSize(), &len);
            pSample->SetActualDataLength(len);
            REFERENCE_TIME rtStart = frameIdx * frameDuration;
            REFERENCE_TIME rtStop = rtStart + frameDuration;
            pSample->SetTime(&rtStart, &rtStop);
            pSample->SetSyncPoint(TRUE);
            pSample->SetDiscontinuity(frameIdx == 0 ? TRUE : FALSE);
            frameIdx++;

            m_pInputPin->Receive(pSample);
        }
        pSample->Release();
    }
}

class COpenCamClassFactory : public IClassFactory {
public:
    COpenCamClassFactory() : m_ref(1) { InterlockedIncrement(&g_dllRefCount); }
    ~COpenCamClassFactory() { InterlockedDecrement(&g_dllRefCount); }

    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID riid, void** ppv) override {
        if (!ppv) return E_POINTER;
        if (riid == IID_IUnknown || riid == IID_IClassFactory) {
            *ppv = static_cast<IClassFactory*>(this);
            AddRef();
            return S_OK;
        }
        *ppv = nullptr;
        return E_NOINTERFACE;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return (ULONG)InterlockedIncrement(&m_ref); }
    ULONG STDMETHODCALLTYPE Release() override {
        ULONG r = (ULONG)InterlockedDecrement(&m_ref);
        if (r == 0) delete this;
        return r;
    }

    HRESULT STDMETHODCALLTYPE CreateInstance(IUnknown* pUnkOuter, REFIID riid, void** ppv) override {
        if (!ppv) return E_POINTER;
        if (pUnkOuter) return CLASS_E_NOAGGREGATION;
        COpenCamFilter* filter = new COpenCamFilter();
        if (!filter) return E_OUTOFMEMORY;
        HRESULT hr = filter->QueryInterface(riid, ppv);
        filter->Release();
        return hr;
    }
    HRESULT STDMETHODCALLTYPE LockServer(BOOL fLock) override {
        if (fLock) InterlockedIncrement(&g_dllRefCount);
        else InterlockedDecrement(&g_dllRefCount);
        return S_OK;
    }

private:
    LONG m_ref;
};

static HRESULT SetRegString(HKEY root, const wchar_t* subkey, const wchar_t* valName, const wchar_t* data) {
    HKEY hKey = nullptr;
    LONG res = RegCreateKeyExW(root, subkey, 0, nullptr, 0, KEY_WRITE, nullptr, &hKey, nullptr);
    if (res != ERROR_SUCCESS) return HRESULT_FROM_WIN32(res);
    DWORD bytes = (DWORD)((wcslen(data) + 1) * sizeof(wchar_t));
    res = RegSetValueExW(hKey, valName, 0, REG_SZ, (const BYTE*)data, bytes);
    RegCloseKey(hKey);
    return HRESULT_FROM_WIN32(res);
}

STDAPI DllGetClassObject(REFCLSID rclsid, REFIID riid, LPVOID* ppv) {
    if (!ppv) return E_POINTER;
    if (rclsid != CLSID_OpenCamVirtualCamera) return CLASS_E_CLASSNOTAVAILABLE;
    COpenCamClassFactory* factory = new COpenCamClassFactory();
    if (!factory) return E_OUTOFMEMORY;
    HRESULT hr = factory->QueryInterface(riid, ppv);
    factory->Release();
    return hr;
}

STDAPI DllCanUnloadNow(void) {
    return (g_dllRefCount == 0) ? S_OK : S_FALSE;
}

STDAPI DllRegisterServer(void) {
    wchar_t dllPath[MAX_PATH] = {};
    if (GetModuleFileNameW(g_hModule, dllPath, MAX_PATH) == 0) {
        return E_FAIL;
    }
    const wchar_t* clsidStr = L"{7A4B9C12-3E5F-4A8B-9C1D-2E3F4A5B6C7D}";
    SetRegString(HKEY_CURRENT_USER, L"Software\\Classes\\CLSID\\{7A4B9C12-3E5F-4A8B-9C1D-2E3F4A5B6C7D}", nullptr, kFilterName);
    SetRegString(HKEY_CURRENT_USER, L"Software\\Classes\\CLSID\\{7A4B9C12-3E5F-4A8B-9C1D-2E3F4A5B6C7D}\\InprocServer32", nullptr, dllPath);
    SetRegString(HKEY_CURRENT_USER, L"Software\\Classes\\CLSID\\{7A4B9C12-3E5F-4A8B-9C1D-2E3F4A5B6C7D}\\InprocServer32", L"ThreadingModel", L"Both");
    SetRegString(HKEY_CURRENT_USER, L"Software\\Classes\\CLSID\\{860BB310-5D01-11d0-BD3B-00A0C911CE86}\\Instance\\{7A4B9C12-3E5F-4A8B-9C1D-2E3F4A5B6C7D}", L"FriendlyName", kFilterName);
    SetRegString(HKEY_CURRENT_USER, L"Software\\Classes\\CLSID\\{860BB310-5D01-11d0-BD3B-00A0C911CE86}\\Instance\\{7A4B9C12-3E5F-4A8B-9C1D-2E3F4A5B6C7D}", L"CLSID", clsidStr);
    return S_OK;
}

STDAPI DllUnregisterServer(void) {
    RegDeleteTreeW(HKEY_CURRENT_USER, L"Software\\Classes\\CLSID\\{860BB310-5D01-11d0-BD3B-00A0C911CE86}\\Instance\\{7A4B9C12-3E5F-4A8B-9C1D-2E3F4A5B6C7D}");
    RegDeleteTreeW(HKEY_CURRENT_USER, L"Software\\Classes\\CLSID\\{7A4B9C12-3E5F-4A8B-9C1D-2E3F4A5B6C7D}");
    return S_OK;
}

BOOL APIENTRY DllMain(HMODULE hModule, DWORD reason, LPVOID) {
    if (reason == DLL_PROCESS_ATTACH) {
        g_hModule = hModule;
        DisableThreadLibraryCalls(hModule);
    }
    return TRUE;
}

