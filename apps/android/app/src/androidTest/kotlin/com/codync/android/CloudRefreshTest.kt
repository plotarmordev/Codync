package com.codync.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import com.codync.android.core.CloudClient
import com.codync.android.core.CloudError
import com.codync.android.core.DeviceIdentity
import com.codync.android.core.RelayCrypto
import java.io.IOException
import java.util.Base64
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import okhttp3.mockwebserver.Dispatcher
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** First-run setup waits on the account's answer: a failed one ends the wait too, or setup stays on its spinner (iOS `aFailedRefreshEndsSetupsWait`). */
class CloudRefreshTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    private fun client(failure: IOException) =
        CloudClient("https://cloud.invalid", DeviceIdentity(ByteArray(32) { 1 }, ByteArray(32) { 2 }), "2.24.0") { throw failure }

    private fun refresh(accounts: AccountStore, client: CloudClient?) = runBlocking(Dispatchers.Main) {
        withTimeout(10_000) { accounts.state.first { "cloud" !in it.busy } }
        accounts.cloud = client
        accounts.mutableState.update { it.copy(cloudLoaded = false) }
        accounts.refreshCloud()
        withTimeout(10_000) { accounts.state.first { it.cloudLoaded && "cloud" !in it.busy } }
    }

    @Test fun theWaitEndsWithoutACloudAndWhenTheRefreshFails() = compose.withPreviewAccounts { accounts ->
        assertTrue(refresh(accounts, null).cloudComputers.isEmpty())
        // A dropped connection passes quietly.
        assertNull(refresh(accounts, client(IOException("offline"))).error)
        // An answer that stays reaches the dialog.
        val refused = refresh(accounts, client(CloudError(403, "forbidden", "Refused")))
        assertEquals("Refused", refused.error)
        assertTrue(refused.cloudComputers.isEmpty())
    }

    private fun client(base: String, token: suspend () -> String = { "token" }) =
        CloudClient(base, DeviceIdentity(ByteArray(32) { 1 }, ByteArray(32) { 2 }), "2.24.0", token)

    /** The account lists a cloud computer as asked while this phone has no request for it, so the store asks again,
     * once; a request that ends unapproved frees the next refresh to ask, and nobody tapped for either, so no dialog
     * says so (iOS `aRefusedCloudRequestIsAskedAgainOnTheNextRefresh`). */
    @Test fun aCloudComputerListedAsAskedWithoutARequestIsAskedAgainAndARefusedOneToo() = compose.withPreviewAccounts { accounts ->
        val key = ByteArray(32) { 6 }
        val base64 = Base64.getUrlEncoder().withoutPadding()
        val id = RelayCrypto.computerId(key)
        val requests = AtomicInteger()
        MockWebServer().use { server ->
            // Every ask shows its code, then is refused: the next refresh asks again.
            val revealed = java.util.concurrent.ConcurrentHashMap.newKeySet<String>()
            val nonce = base64.encodeToString(ByteArray(32) { 9 })
            server.dispatcher = object : Dispatcher() {
                override fun dispatch(request: RecordedRequest): MockResponse {
                    val path = request.path.orEmpty()
                    return when {
                        path.endsWith("/v1/computers") -> MockResponse().setBody("""{"computers":[{"computerId":"$id","name":"Cloud","signKey":"${base64.encodeToString(key)}","access":"pending","cloud":{"provider":"boat","providerName":"Boat","region":"EU","state":"running"}}]}""")
                        path.endsWith("/access-requests") -> MockResponse().setBody("""{"requestId":"r${requests.incrementAndGet()}","expiresAt":4102444800000}""")
                        path.contains("/reveal") -> { revealed += path.substringBeforeLast('/').substringAfterLast('/'); MockResponse().setBody("{}") }
                        path.contains("/access-requests/") -> {
                            val rid = path.substringAfterLast('/')
                            val body = if (rid in revealed) """{"requestId":"$rid","status":"denied"}""" else """{"requestId":"$rid","status":"pending","hostNonce":"$nonce"}"""
                            MockResponse().setBody(body)
                        }
                        else -> MockResponse().setBody("{}")
                    }
                }
            }
            runBlocking(Dispatchers.Main) {
                accounts.cloud = client(server.url("/").toString())
                accounts.refreshCloud()
                // The request that ends refused doesn't leave a dialog: nobody tapped for it.
                withTimeout(10_000) { accounts.state.first { requests.get() == 1 && "access:$id" !in it.busy && id !in it.access } }
                assertNull(accounts.state.value.error)
                val first = requests.get()
                accounts.refreshCloud()
                withTimeout(10_000) { accounts.state.first { requests.get() > first && "access:$id" !in it.busy && id !in it.access } }
                assertNull(accounts.state.value.error)
            }
        }
    }
}
